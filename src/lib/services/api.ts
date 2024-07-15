interface EchoResponse {
  message: string;
  correlationId: string | null;
  intent?: string;
  slider?: string;
}

export async function sendEchoRequest(message: string, correlationId: string | null): Promise<EchoResponse> {
  const response = await fetch('http://localhost:3000/api/echo', {
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
