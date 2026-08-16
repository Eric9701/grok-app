/**
 * ConversationThread bound to sessionTranscriptStore.
 * Isolates stream token re-renders from the App shell.
 */

import { ConversationThread, type ConversationThreadProps } from "./ConversationThread";
import {
  useTranscriptMeta,
  useViewingMessages,
} from "@/hooks/useSessionTranscript";

export type ConversationThreadLiveProps = Omit<
  ConversationThreadProps,
  "messages"
>;

export function ConversationThreadLive(props: ConversationThreadLiveProps) {
  const messages = useViewingMessages();
  const meta = useTranscriptMeta();
  const journalLoading = !!props.journalLoading || meta.journalLoading;
  return (
    <ConversationThread
      {...props}
      messages={messages}
      journalLoading={journalLoading}
    />
  );
}
