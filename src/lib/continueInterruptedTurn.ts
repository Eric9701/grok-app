/** Build the agent-facing continue prompt after a host/agent interrupt. */

export interface ContinueInterruptContext {
  command?: string | null;
  title?: string | null;
  toolName?: string | null;
}

export const CONTINUE_JOURNAL_PHRASE_KEY = "endOfTurn.continuePrompt" as const;

export function buildContinueAgentPrompt(
  ctx: ContinueInterruptContext | null | undefined,
): string {
  const command = ctx?.command?.trim() ?? "";
  const title = ctx?.title?.trim() ?? "";
  const toolName = ctx?.toolName?.trim() ?? "";
  const lines = [
    "The previous turn was interrupted when the app host process restarted.",
    "Do not redo steps that already succeeded. Continue the user's last request from the interrupted tool call.",
    "Do not assume a previous permission prompt is still open — request approval again if you need to run a command.",
  ];
  if (toolName) {
    lines.push(`Interrupted tool: ${toolName}`);
  }
  if (title) {
    lines.push(`Tool title: ${title}`);
  }
  if (command) {
    lines.push("Unfinished command:");
    lines.push("```");
    lines.push(command);
    lines.push("```");
  } else {
    lines.push(
      "The unfinished command is not available. Use the conversation history to resume from the last incomplete step.",
    );
  }
  return lines.join("\n");
}

export function isContinuableEndReason(reason: string | null | undefined): boolean {
  const r = (reason || "").toLowerCase();
  return r === "host_exit" || r === "agent_exit";
}
