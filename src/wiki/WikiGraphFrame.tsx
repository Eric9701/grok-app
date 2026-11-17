import { Component, lazy, Suspense, type ReactNode } from "react";
import type { MessageKey } from "@/i18n";

const WikiGraph = lazy(() => import("./WikiGraph").then((mod) => ({ default: mod.WikiGraph })));

export function WikiGraphFrame({
  projectPath,
  pages,
  tr,
  onOpen,
}: {
  projectPath: string;
  pages: string[];
  tr: (key: MessageKey) => string;
  onOpen: (path: string) => void;
}) {
  return (
    <GraphBoundary message={tr("wiki.graphFailed")}>
      <Suspense fallback={<p className="wiki-graph__note">{tr("wiki.loading")}</p>}>
        <WikiGraph projectPath={projectPath} pages={pages} tr={tr} onOpen={onOpen} />
      </Suspense>
    </GraphBoundary>
  );
}

class GraphBoundary extends Component<{ message: string; children: ReactNode }, { failed: boolean }> {
  state = { failed: false };

  static getDerivedStateFromError(): { failed: boolean } {
    return { failed: true };
  }

  render() {
    if (this.state.failed) return <p className="wiki-graph__note">{this.props.message}</p>;
    return this.props.children;
  }
}
