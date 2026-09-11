import assert from 'node:assert/strict'
// eslint-disable-next-line test/no-import-node-test -- Use the built-in runner without a new dependency.
import { it } from 'node:test'
import { readConnectionTestEvents } from '../src/api/connection-test-stream.ts'

function byteStream(text, chunkSize = 1) {
  const bytes = new TextEncoder().encode(text)
  let offset = 0
  return new ReadableStream({
    pull(controller) {
      if (offset >= bytes.length) {
        controller.close()
        return
      }
      controller.enqueue(bytes.slice(offset, offset + chunkSize))
      offset += chunkSize
    },
  })
}

it('reads UTF-8 split across every byte and CRLF across chunks', async () => {
  const events = []
  await readConnectionTestEvents(byteStream(': keepalive\r\n\r\ndata: {"text":"\u4F60\u597D"}\r\n\r\ndata: {"type":"test_complete","success":true}\n\n'), (raw) => {
    events.push(JSON.parse(raw))
    return true
  })
  assert.deepEqual(events, [{ text: '\u4F60\u597D' }, { type: 'test_complete', success: true }])
})

it('combines data fields, ignores unknown fields and handles multiple frames per chunk', async () => {
  const events = []
  await readConnectionTestEvents(byteStream('event: ignored\nid: 4\ndata: {"a":\ndata: 1}\n\ndata:{"b":2}\n\n', 4096), (raw) => {
    events.push(JSON.parse(raw))
    return true
  })
  assert.deepEqual(events, [{ a: 1 }, { b: 2 }])
})

it('stops at the terminal event and cancels the reader without retrying', async () => {
  let cancelled = false
  const events = []
  const stream = new ReadableStream({
    start(controller) {
      controller.enqueue(new TextEncoder().encode('data: done\n\ndata: stale\n\n'))
    },
    cancel() { cancelled = true },
  })
  await readConnectionTestEvents(stream, (raw) => {
    events.push(raw)
    return false
  })
  assert.deepEqual(events, ['done'])
  assert.equal(cancelled, true)
  assert.equal(stream.locked, false)
})

it('does not treat an incomplete final frame as a successful response', async () => {
  const events = []
  await readConnectionTestEvents(byteStream('data: {"type":"test_complete","success":true}\n'), (raw) => {
    events.push(raw)
    return true
  })
  assert.deepEqual(events, [])
})

it('rejects oversized unterminated and multiline events', async () => {
  for (const payload of [`data: ${'x'.repeat(1_048_577)}`, (`data: ${'x'.repeat(4096)}\n`).repeat(257)]) {
    await assert.rejects(readConnectionTestEvents(byteStream(payload, 4096), () => true), /too large/)
  }
})

it('propagates malformed JSON and stream failures while releasing the reader', async () => {
  const malformed = byteStream('data: {broken}\n\n')
  await assert.rejects(readConnectionTestEvents(malformed, JSON.parse), SyntaxError)
  assert.equal(malformed.locked, false)
  const failed = new ReadableStream({
    start(controller) {
      controller.error(new Error('disconnected'))
    },
  })
  await assert.rejects(readConnectionTestEvents(failed, () => true), /disconnected/)
  assert.equal(failed.locked, false)
})
