import type { Attachment } from "@/lib/attachments";

type AttachmentPath = Pick<Attachment, "path">;

function attachmentsMatchForSubmit(
  sent: AttachmentPath[],
  current: AttachmentPath[],
): boolean {
  if (sent.length !== current.length) return false;
  return sent.every((item, i) => item.path === current[i]?.path);
}

/**
 * After a successful send, clear the composer only when the live draft is
 * still the payload that was submitted. New keystrokes / attachments during
 * a multi-second `executeSend` must not be wiped (#599: fail keeps draft;
 * success must not swallow follow-up input).
 */
export function shouldClearComposerAfterSubmit(opts: {
  sentText: string;
  sentAttachments: AttachmentPath[];
  currentText: string;
  currentAttachments: AttachmentPath[];
}): boolean {
  return (
    opts.currentText === opts.sentText &&
    attachmentsMatchForSubmit(opts.sentAttachments, opts.currentAttachments)
  );
}
