import { StrictMode, Suspense, lazy } from "react";
import ReactDOM from "react-dom/client";
const App = lazy(() => import("./App"));
const QuickApp = lazy(() => import("./QuickApp").then(m => ({ default: m.QuickApp })));
import "./styles.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <StrictMode>
    <Suspense fallback={<div className="flex h-dvh items-center justify-center bg-canvas text-xs text-muted" role="status">불러오는 중</div>}>
      {location.hash === "#/quick" ? <QuickApp /> : <App />}
    </Suspense>
  </StrictMode>
);
