/**
 * @author kongweiguang
 * 应用根组件，只负责挂载错误边界和主工作台。
 */

import "./App.css";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { Workbench } from "./features/workbench/Workbench";

/** Net Power 前端入口。 */
function App() {
  return (
    <ErrorBoundary>
      <Workbench />
    </ErrorBoundary>
  );
}

export default App;
