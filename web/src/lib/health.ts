export type HealthResponse = {
  status: 'ok'
  name: 'pi-trace'
  version: string
  stage: 'bootstrap'
}

function decodeHealth(value: unknown): HealthResponse {
  if (
    typeof value !== 'object' ||
    value === null ||
    !('status' in value) ||
    value.status !== 'ok' ||
    !('name' in value) ||
    value.name !== 'pi-trace' ||
    !('version' in value) ||
    typeof value.version !== 'string' ||
    value.version.length === 0 ||
    !('stage' in value) ||
    value.stage !== 'bootstrap'
  ) {
    throw new Error('Unexpected health response')
  }

  return {
    status: value.status,
    name: value.name,
    version: value.version,
    stage: value.stage,
  }
}

export async function getHealth(): Promise<HealthResponse> {
  const response = await fetch('/api/v1/health', {
    signal: AbortSignal.timeout(5000),
    cache: 'no-store',
  })

  if (!response.ok) {
    throw new Error('Health request failed')
  }

  const body: unknown = await response.json()
  return decodeHealth(body)
}
