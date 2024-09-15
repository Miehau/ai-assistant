interface EchoResponse {
  message: string;
  correlationId: string | null;
  intent?: string;
  slider?: string;
}

const API_URL = 'http://localhost:3000'; // Make sure this matches your backend port

export async function sendEchoRequest(message: string, correlationId: string | null): Promise<EchoResponse> {
  const response = await fetch(`${API_URL}/api/chat`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
    },
    body: JSON.stringify({
      message,
      correlationId,
    }),
  });

  if (!response.ok) {
    throw new Error('Failed to send echo request');
  }

  return await response.json();
}

export async function sendChatMessage(message: string): Promise<{ text: string }> {
  const response = await fetch(`${API_URL}/api/chat`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
    },
    body: JSON.stringify({ input: message }),
  });

  if (!response.ok) {
    console.error('Failed to send chat message:', response);
    throw new Error('Failed to send chat message');
  }

  return response.json();
}
