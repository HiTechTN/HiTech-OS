import { useState } from "react";

import { GenioProvider, useGenio } from "./state/GenioProvider";
import { Shell, type TabId } from "./components/Shell";
import { ApprovalModal } from "./components/ApprovalModal";
import { AuditView } from "./components/AuditView";
import { ConsoleView } from "./components/Console";
import { PolicyView } from "./components/PolicyView";
import { TelemetryView } from "./components/TelemetryView";
import { ToolsView } from "./components/ToolsView";
import { ErrorBanner } from "./components/ui";

function Dashboard() {
  const { health, status, connected, pending, error, clearError } = useGenio();
  const [tab, setTab] = useState<TabId>("console");

  return (
    <Shell
      tab={tab}
      onTab={setTab}
      health={health}
      status={status}
      connected={connected}
      pendingCount={pending.length}
    >
      <div className="space-y-4">
        {error ? <ErrorBanner message={error} onClose={clearError} /> : null}
        {tab === "console" ? <ConsoleView /> : null}
        {tab === "telemetry" ? <TelemetryView /> : null}
        {tab === "policy" ? <PolicyView /> : null}
        {tab === "tools" ? <ToolsView /> : null}
        {tab === "audit" ? <AuditView /> : null}
      </div>
      <ApprovalModal />
    </Shell>
  );
}

export default function App() {
  return (
    <GenioProvider>
      <Dashboard />
    </GenioProvider>
  );
}
