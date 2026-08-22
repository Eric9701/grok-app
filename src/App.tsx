/**
 * App shell — providers only. Feature state lives in domain modules.
 * Growth freeze: App.tsx + AppWorkbench.tsx combined lines may only decrease
 * (see AGENTS.md §7). New product state must not land here.
 */
import { ThemeProvider } from "@/providers/ThemeProvider";
import { SkinShareProvider } from "@/providers/SkinShareProvider";
import { AppWorkbench } from "@/app/AppWorkbench";

export default function App() {
  return (
    <ThemeProvider>
      <SkinShareProvider>
        <AppWorkbench />
      </SkinShareProvider>
    </ThemeProvider>
  );
}
