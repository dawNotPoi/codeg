import type { DbConversationDetail } from "@/lib/types"
import { routeAcpError, type AcpErrorLevel } from "@/lib/acp-error-presentation"
import { resolveVisibleConversationError } from "@/lib/conversation-error"

export interface PersistedConversationErrorAlert {
  key: string
  level: AcpErrorLevel
  message: string
}

export function selectPersistedConversationErrorAlert({
  conversationId,
  detail,
  liveError,
  status,
  retiredRevision,
}: {
  conversationId: number | null
  detail: DbConversationDetail | null
  liveError: string | null
  status: string | null
  retiredRevision: number | null
}): PersistedConversationErrorAlert | null {
  if (
    conversationId == null ||
    detail?.summary.id !== conversationId ||
    liveError
  ) {
    return null
  }
  const message = resolveVisibleConversationError(
    null,
    status,
    detail,
    retiredRevision
  )
  if (!message) return null

  const route = routeAcpError(detail.last_error?.code)
  if (route.kind === "transcript") return null
  return {
    key: `persisted-acp-error:${conversationId}:${detail.last_error_revision ?? 0}`,
    level: route.level,
    message,
  }
}

/** Shared across mounted views so opening the same history in another tab
 * does not replay its recovered alert. A new revision can still be shown. */
export class PersistedConversationErrorAlertTracker {
  private readonly seen = new Set<string>()

  claim(key: string): boolean {
    if (this.seen.has(key)) return false
    this.seen.add(key)
    return true
  }
}

export const persistedConversationErrorAlertTracker =
  new PersistedConversationErrorAlertTracker()
