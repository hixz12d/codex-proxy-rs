// The admin endpoint emits JSON in SSE data fields with LF or CRLF framing.
export async function readConnectionTestEvents(
  stream: ReadableStream<Uint8Array>,
  onMessage: (data: string) => boolean,
) {
  const reader = stream.getReader()
  const decoder = new TextDecoder('utf-8', { fatal: true })
  let buffer = ''
  let data: string[] = []
  let eventSize = 0
  try {
    while (true) {
      const { value, done } = await reader.read()
      buffer += done ? decoder.decode() : decoder.decode(value, { stream: true })
      while (buffer.includes('\n')) {
        const end = buffer.indexOf('\n')
        const line = buffer.slice(0, end).replace(/\r$/, '')
        buffer = buffer.slice(end + 1)
        if (line === '') {
          if (data.length && !onMessage(data.join('\n')))
            return
          data = []
          eventSize = 0
        }
        else if (line === 'data' || line.startsWith('data:')) {
          const text = line === 'data' ? '' : line.slice(5).replace(/^ /, '')
          eventSize += text.length
          if (eventSize > 1_048_576)
            throw new Error('Connection test event is too large')
          data.push(text)
        }
      }
      if (buffer.length > 1_048_576)
        throw new Error('Connection test event is too large')
      if (done)
        return
    }
  }
  finally {
    await reader.cancel().catch(() => {})
    reader.releaseLock()
  }
}
