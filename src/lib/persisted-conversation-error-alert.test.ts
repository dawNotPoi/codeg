import { describe, expect, it } from "vitest"
import type { DbConversationDetail } from "@/lib/types"
import {
  PersistedConversationErrorAlertTracker,
  selectPersistedConversationErrorAlert,
} from "@/lib/persisted-conversation-error-alert"

function detail(
  id: number,
  revision: number,
  message: string,
  code: string | null = null
): DbConversationDetail {
  return {
    summary: { id } as DbConversationDetail["summary"],
    last_error: { message, code, details: null },
    last_error_revision: revision,
    turns: [],
  }
}

describe("persisted conversation error alerts", () => {
  it("shows a cold-loaded history error once and keeps it scoped to its conversation", () => {
    const tracker = new PersistedConversationErrorAlertTracker()
    const first = selectPersistedConversationErrorAlert({
      conversationId: 17,
      detail: detail(17, 3, "history failed"),
      liveError: null,
      status: "disconnected",
      retiredRevision: null,
    })
    expect(first).toEqual({
      key: "persisted-acp-error:17:3",
      level: "error",
      message: "history failed",
    })
    expect(tracker.claim(first!.key)).toBe(true)
    expect(tracker.claim(first!.key)).toBe(false)
    expect(
      selectPersistedConversationErrorAlert({
        conversationId: 18,
        detail: detail(17, 3, "history failed"),
        liveError: null,
        status: "disconnected",
        retiredRevision: null,
      })
    ).toBeNull()
    const second = selectPersistedConversationErrorAlert({
      conversationId: 18,
      detail: detail(18, 1, "other failure"),
      liveError: null,
      status: "disconnected",
      retiredRevision: null,
    })
    expect(second?.key).toBe("persisted-acp-error:18:1")
    expect(tracker.claim(second!.key)).toBe(true)
  })

  it("skips the live error and retires the old detail on a new prompt", () => {
    const old = detail(17, 3, "old failure")
    const input = {
      conversationId: 17,
      detail: old,
      liveError: "old failure",
      status: "connected",
      retiredRevision: null,
    }
    expect(selectPersistedConversationErrorAlert(input)).toBeNull()
    expect(
      selectPersistedConversationErrorAlert({
        ...input,
        liveError: null,
        status: "prompting",
      })
    ).toBeNull()
    expect(
      selectPersistedConversationErrorAlert({
        ...input,
        liveError: null,
        retiredRevision: 3,
      })
    ).toBeNull()
    expect(
      selectPersistedConversationErrorAlert({
        ...input,
        detail: detail(17, 4, "new failure"),
        liveError: null,
        retiredRevision: 3,
      })?.key
    ).toBe("persisted-acp-error:17:4")
  })

  it("follows upstream routing for warning and transcript-only errors", () => {
    expect(
      selectPersistedConversationErrorAlert({
        conversationId: 17,
        detail: detail(17, 1, "lost context", "session_load_fallback"),
        liveError: null,
        status: "connected",
        retiredRevision: null,
      })?.level
    ).toBe("warning")
    expect(
      selectPersistedConversationErrorAlert({
        conversationId: 17,
        detail: detail(17, 2, "already in transcript", "compaction_failed"),
        liveError: null,
        status: "connected",
        retiredRevision: null,
      })
    ).toBeNull()
  })
})
