import { beforeEach, describe, expect, it, vi } from "vitest"
import { RemoteDesktopTransport } from "./remote-desktop-transport"

const tauri = vi.hoisted(() => ({
  invoke: vi.fn(),
  listen: vi.fn(),
}))

vi.mock("@tauri-apps/api/core", () => ({ invoke: tauri.invoke }))
vi.mock("@tauri-apps/api/event", () => ({ listen: tauri.listen }))

let deliver: (frame: unknown) => void

beforeEach(() => {
  tauri.invoke.mockReset().mockResolvedValue(undefined)
  tauri.listen.mockReset().mockImplementation(async (_name, handler) => {
    deliver = (frame) => handler({ payload: frame })
    return () => {}
  })
})

describe("RemoteDesktopTransport reconnect", () => {
  it("reissues live attach subscriptions when the Rust WS becomes ready again", async () => {
    const transport = new RemoteDesktopTransport({
      id: 7,
      name: "remote",
      baseUrl: "http://localhost:3000",
      token: "token",
      windowInstanceId: "window-1",
    })
    const reconnect = vi.fn()
    transport.onReconnect(reconnect)
    const sub = transport.eventStream().attach(
      "connection-1",
      {},
      {
        onSnapshot: vi.fn(),
        onReplay: vi.fn(),
        onEvent: vi.fn(),
        onDetached: vi.fn(),
      }
    )

    await vi.waitFor(() => expect(tauri.listen).toHaveBeenCalledOnce())
    const ready = { channel: "__ready__", payload: null }
    deliver(ready)

    const attachFrames = () =>
      tauri.invoke.mock.calls
        .filter(([command]) => command === "remote_ws_send_text")
        .map(([, args]) => JSON.parse(args.text))
    expect(attachFrames()).toEqual([
      expect.objectContaining({
        action: "attach",
        subscription_id: sub.subscriptionId,
      }),
    ])

    deliver({ channel: "__disconnected__", payload: null })
    deliver(ready)
    expect(attachFrames()).toEqual([
      expect.objectContaining({
        action: "attach",
        subscription_id: sub.subscriptionId,
      }),
      expect.objectContaining({
        action: "attach",
        subscription_id: sub.subscriptionId,
      }),
    ])
    expect(reconnect).toHaveBeenCalledOnce()
    transport.destroy()
  })
})
