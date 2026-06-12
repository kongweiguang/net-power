/**
 * @author kongweiguang
 */

import { Component, type ErrorInfo, type ReactNode } from "react";

/** 错误边界属性。 */
interface ErrorBoundaryProps {
  /** 受保护的 React 子树。 */
  children: ReactNode;
}

/** 错误边界状态。 */
interface ErrorBoundaryState {
  /** 是否捕获到渲染错误。 */
  hasError: boolean;
  /** 捕获到的错误对象。 */
  error: Error | null;
}

/** 捕获渲染期异常，避免桌面应用白屏。 */
export class ErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  state: ErrorBoundaryState = {
    hasError: false,
    error: null,
  };

  static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return {
      hasError: true,
      error,
    };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("工作台渲染异常", error, info);
  }

  render() {
    if (this.state.hasError) {
      return (
        <main className="fatal-screen">
          <section className="fatal-panel">
            <p className="eyebrow">Workspace Error</p>
            <h1>工作台渲染失败</h1>
            <p>{this.state.error?.message || "发生未知前端错误，请刷新页面后重试。"}</p>
            <button type="button" className="primary-button" onClick={() => window.location.reload()}>
              刷新工作台
            </button>
          </section>
        </main>
      );
    }

    return this.props.children;
  }
}
