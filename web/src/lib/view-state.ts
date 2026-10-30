export type Filter = '' | 'error' | 'tool' | 'generation' | 'slow'
export type ExecutionView = 'waterfall' | 'list'
export type InspectorTab = 'details' | 'metadata' | 'context'
export type Theme = 'light' | 'dark'
export type ViewState = {
  snapshot: string | null
  node: string | null
  filter: Filter
  view: ExecutionView
  tab: InspectorTab
  theme: Theme
}

export function readViewState(url: URL): ViewState {
  const params = url.searchParams
  const filter = params.get('filter')
  const snapshot = params.get('snapshot')
  return {
    snapshot,
    node: snapshot ? params.get('node') : null,
    filter: filter === 'error' || filter === 'tool' || filter === 'generation' || filter === 'slow' ? filter : '',
    view: params.get('view') === 'list' ? 'list' : 'waterfall',
    tab: params.get('tab') === 'metadata' ? 'metadata' : params.get('tab') === 'context' ? 'context' : 'details',
    theme: params.get('theme') === 'light' ? 'light' : 'dark',
  }
}

export function viewUrl(state: ViewState, url: URL, sessionId: string | null = null): URL {
  url.search = ''
  if (sessionId) url.searchParams.set('session', sessionId)
  if (state.snapshot) url.searchParams.set('snapshot', state.snapshot)
  if (state.node) url.searchParams.set('node', state.node)
  if (state.filter) url.searchParams.set('filter', state.filter)
  url.searchParams.set('view', state.view)
  url.searchParams.set('tab', state.tab)
  url.searchParams.set('theme', state.theme)
  url.hash = ''
  return url
}
