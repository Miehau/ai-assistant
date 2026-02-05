use super::{
    BranchOperations, ConversationOperations, CreateIntegrationConnectionInput,
    CreateMcpServerInput, Db, DbOperations, IncomingAttachment, IntegrationConnectionOperations,
    McpServerOperations, MessageOperations, Model, ModelOperations, PreferenceOperations,
    UpdateIntegrationConnectionInput, UpdateMcpServerInput,
};
use rusqlite::params;
use uuid::Uuid;

fn setup_db() -> Db {
    let db_path = std::env::temp_dir().join(format!("ai-agent-test-{}.db", Uuid::new_v4()));
    let mut db = Db::new(db_path.to_str().unwrap()).expect("db init failed");
    db.run_migrations().expect("db migrations failed");
    db
}

#[test]
fn preferences_round_trip() {
    let db = setup_db();

    assert!(db.get_preference("missing.pref").unwrap().is_none());

    db.set_preference("ui.theme", "light").unwrap();
    assert_eq!(
        db.get_preference("ui.theme").unwrap().as_deref(),
        Some("light")
    );

    db.set_preference("ui.theme", "dark").unwrap();
    assert_eq!(
        db.get_preference("ui.theme").unwrap().as_deref(),
        Some("dark")
    );
}

#[test]
fn models_and_api_keys_crud() {
    let db = setup_db();

    let model = Model {
        provider: "openai".to_string(),
        model_name: "gpt-4o".to_string(),
        url: Some("https://example.com".to_string()),
        deployment_name: None,
        enabled: true,
        custom_backend_id: None,
    };

    db.add_model(&model).unwrap();
    let models = db.get_models().unwrap();
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].provider, "openai");
    assert!(models[0].enabled);

    db.toggle_model("openai", "gpt-4o").unwrap();
    let models = db.get_models().unwrap();
    assert_eq!(models.len(), 1);
    assert!(!models[0].enabled);

    db.delete_model("openai", "gpt-4o").unwrap();
    assert!(db.get_models().unwrap().is_empty());

    db.set_api_key("openai", "secret-key").unwrap();
    assert_eq!(
        db.get_api_key("openai").unwrap().as_deref(),
        Some("secret-key")
    );
    db.delete_api_key("openai").unwrap();
    assert!(db.get_api_key("openai").unwrap().is_none());
}

#[test]
fn save_message_rewrites_invalid_id() {
    let db = setup_db();
    let conversation_id = "conv-invalid-id";
    db.get_or_create_conversation(conversation_id).unwrap();

    let invalid_id = "not-a-uuid";
    let message_id = db
        .save_message(
            conversation_id,
            "user",
            "hello",
            &[],
            Some(invalid_id.to_string()),
        )
        .unwrap();

    assert_ne!(message_id, invalid_id);
    assert!(Uuid::parse_str(&message_id).is_ok());
}

#[test]
fn conversation_delete_removes_messages_and_attachments() {
    let db = setup_db();
    let conversation_id = "conv-delete";
    db.get_or_create_conversation(conversation_id).unwrap();

    let attachments = vec![IncomingAttachment {
        name: "note.txt".to_string(),
        data: "Hello".to_string(),
        attachment_type: "text/plain".to_string(),
        description: None,
        transcript: None,
    }];

    let message_id = db
        .save_message(conversation_id, "user", "hello", &attachments, None)
        .unwrap();

    {
        let binding = db.conn();
        let conn = binding.lock().unwrap();
        let message_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE conversation_id = ?1",
                params![conversation_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(message_count, 1);

        let attachment_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM message_attachments WHERE message_id = ?1",
                params![message_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(attachment_count, 1);
    }

    db.delete_conversation(conversation_id).unwrap();

    {
        let binding = db.conn();
        let conn = binding.lock().unwrap();
        let conversation_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM conversations WHERE id = ?1",
                params![conversation_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(conversation_count, 0);

        let message_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE conversation_id = ?1",
                params![conversation_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(message_count, 0);

        let attachment_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM message_attachments WHERE message_id = ?1",
                params![message_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(attachment_count, 0);
    }
}

#[test]
fn create_branch_from_message_copies_path_and_marks_branch_point() {
    let db = setup_db();
    let conversation_id = "conv-branch";
    db.get_or_create_conversation(conversation_id).unwrap();

    let message_ids = vec![
        Uuid::new_v4().to_string(),
        Uuid::new_v4().to_string(),
        Uuid::new_v4().to_string(),
    ];

    for (index, message_id) in message_ids.iter().enumerate() {
        db.save_message(
            conversation_id,
            "user",
            &format!("message {}", index + 1),
            &[],
            Some(message_id.clone()),
        )
        .unwrap();
    }

    let main_branch = db.get_or_create_main_branch(conversation_id).unwrap();

    db.create_message_tree_node(&message_ids[0], None, &main_branch.id, false)
        .unwrap();
    db.create_message_tree_node(
        &message_ids[1],
        Some(&message_ids[0]),
        &main_branch.id,
        false,
    )
    .unwrap();
    db.create_message_tree_node(
        &message_ids[2],
        Some(&message_ids[1]),
        &main_branch.id,
        false,
    )
    .unwrap();

    let new_branch = db
        .create_branch_from_message(conversation_id, &message_ids[1], "Fork")
        .unwrap();

    {
        let binding = db.conn();
        let conn = binding.lock().unwrap();

        let branch_point: i64 = conn
            .query_row(
                "SELECT branch_point FROM message_tree WHERE message_id = ?1 AND branch_id = ?2",
                params![message_ids[1], main_branch.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(branch_point, 1);

        let mut stmt = conn
            .prepare("SELECT message_id FROM message_tree WHERE branch_id = ?1")
            .unwrap();
        let branch_ids: std::collections::HashSet<String> = stmt
            .query_map(params![new_branch.id], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        let expected: std::collections::HashSet<String> =
            message_ids[..=1].iter().cloned().collect();
        assert_eq!(branch_ids, expected);
    }
}

#[test]
fn mcp_servers_crud() {
    let db = setup_db();

    let server = db
        .create_mcp_server(&CreateMcpServerInput {
            name: "Local MCP".to_string(),
            url: "http://localhost:3000".to_string(),
            auth_type: "api_key".to_string(),
            api_key: Some("secret".to_string()),
        })
        .unwrap();

    let servers = db.get_mcp_servers().unwrap();
    assert_eq!(servers.len(), 1);
    assert_eq!(servers[0].name, "Local MCP");

    let updated = db
        .update_mcp_server(&UpdateMcpServerInput {
            id: server.id.clone(),
            name: Some("Updated MCP".to_string()),
            url: None,
            auth_type: None,
            api_key: None,
        })
        .unwrap()
        .expect("updated server");
    assert_eq!(updated.name, "Updated MCP");

    let deleted = db.delete_mcp_server(&server.id).unwrap();
    assert!(deleted);
}

#[test]
fn integration_connections_crud() {
    let db = setup_db();

    let connection = db
        .create_integration_connection(&CreateIntegrationConnectionInput {
            integration_id: "gmail".to_string(),
            account_label: Some("Work Gmail".to_string()),
            auth_type: "oauth2".to_string(),
            access_token: Some("access-token".to_string()),
            refresh_token: Some("refresh-token".to_string()),
            scopes: Some("scope-a scope-b".to_string()),
            expires_at: Some(1234567890),
        })
        .unwrap();

    assert_eq!(connection.status, "connected");

    let updated = db
        .update_integration_connection(&UpdateIntegrationConnectionInput {
            id: connection.id.clone(),
            account_label: Some("Personal Gmail".to_string()),
            status: Some("error".to_string()),
            auth_type: None,
            access_token: None,
            refresh_token: None,
            scopes: None,
            expires_at: None,
            last_error: Some("HTTP 401".to_string()),
            last_sync_at: Some(999999),
        })
        .unwrap()
        .expect("updated connection");

    assert_eq!(updated.account_label.as_deref(), Some("Personal Gmail"));
    assert_eq!(updated.status, "error");

    let deleted = db.delete_integration_connection(&connection.id).unwrap();
    assert!(deleted);
}

#[test]
fn repair_message_tree_adds_orphaned_messages() {
    let db = setup_db();
    let conversation_id = "conv-repair";
    db.get_or_create_conversation(conversation_id).unwrap();

    let message_id = db
        .save_message(conversation_id, "user", "hello", &[], None)
        .unwrap();

    let consistency = db.check_message_tree_consistency().unwrap();
    assert_eq!(consistency.orphaned_count, 1);
    assert!(consistency.orphaned_messages.contains(&message_id));
    assert!(!consistency.is_consistent);

    let repaired = db.repair_message_tree().unwrap();
    assert_eq!(repaired, 1);

    let repaired_consistency = db.check_message_tree_consistency().unwrap();
    assert_eq!(repaired_consistency.orphaned_count, 0);
    assert!(repaired_consistency.is_consistent);
}
