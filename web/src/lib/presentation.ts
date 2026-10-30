import type { Measurement, TokenCounter } from './trace-api'

export function milliseconds(value: number): string {
  return value >= 1000 ? `${(value / 1000).toLocaleString(undefined, { maximumFractionDigits: 2 })}s` : `${value.toLocaleString(undefined, { maximumFractionDigits: 1 })}ms`
}
export function timing(measurement: Measurement): string {
  return measurement.valueMs === null ? measurement.state : milliseconds(measurement.valueMs)
}
export function quantity(value: number | string | null): string {
  return value === null ? 'Unknown' : typeof value === 'string' ? BigInt(value).toLocaleString() : value.toLocaleString()
}

export function tokens(counter: TokenCounter): string {
  switch (counter.state) {
    case 'exact': return quantity(counter.value)
    case 'partial': return `${quantity(counter.value)} (partial subtotal)`
    case 'unknown': return 'Unknown'
    case 'invalid': return 'Invalid'
    case 'notApplicable': return 'Not applicable'
  }
}

export function sourceLabel(source: import('./trace-api').Source): string {
  if ('file' in source) return `${source.file}:${source.line}`
  const namespace = `${source.provider} · store ${source.storeId}`
  switch (source.kind) {
    case 'json': return `${namespace} · ${source.source} · JSON ${source.pointer === null ? 'document' : source.pointer === '' ? 'root' : source.pointer}`
    case 'jsonl': return `${namespace} · ${source.source}:${source.line}${source.byteRange ? ` · bytes ${source.byteRange.start}–${source.byteRange.end}` : ''}`
    case 'compressedJsonl': return `${namespace} · ${source.source} · ${source.encoding} decoded line ${source.decodedLine}${source.decodedByteRange ? ` · decoded bytes ${source.decodedByteRange.start}–${source.decodedByteRange.end}` : ''}`
    case 'sqlite': {
      const key = source.primaryKey.map((item) => `${item.column}=${item.value.kind === 'blob' ? `blob(${item.value.hex})` : item.value.value}`).join(', ')
      const field = source.field ? source.field.kind === 'json' ? `${source.field.column}${source.field.pointer}` : source.field.column : 'whole row'
      return `${namespace} · ${source.database} · ${source.schemaProfile} · ${source.table} [${key || 'no recorded row key'}] · ${field} · revision ${source.observedRevision}`
    }
  }
}
