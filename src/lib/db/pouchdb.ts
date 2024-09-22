import PouchDB from 'pouchdb';
import { browser } from '$app/environment';

let localDB: PouchDB.Database | null = null;
let remoteDB: PouchDB.Database | null = null;

if (import.meta.env.VITE_DEPLOYMENT_TARGET === 'tauri') {
  localDB = new PouchDB('ai_backend_local_db');

  remoteDB = new PouchDB(import.meta.env.VITE_COUCHDB_URL as string, {
    auth: {
      username: import.meta.env.VITE_COUCHDB_USERNAME as string,
      password: import.meta.env.VITE_COUCHDB_PASSWORD as string,
    },
  });

  // Set up synchronization
  localDB.sync(remoteDB, {
    live: true,
    retry: true,
  })
    .on('change', (info) => {
      console.log('Synchronization change:', info);
    })
    .on('paused', (err) => {
      console.log('Synchronization paused:', err);
    })
    .on('active', () => {
      console.log('Synchronization resumed');
    })
    .on('error', (err) => {
      console.error('Synchronization error:', err);
    });
}

export { localDB, remoteDB };