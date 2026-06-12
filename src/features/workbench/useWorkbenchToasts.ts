/**
 * @author kongweiguang
 * Workbench toast 状态管理。
 */

import { useCallback, useEffect, useRef, useState } from "react";
import type { Toast } from "./workbenchShared";

/** 触发一条工作台 toast。 */
export type ToastFn = (kind: Toast["kind"], title: string, detail?: string) => void;

/** 管理工作台 toast 队列和自动关闭定时器。 */
export function useWorkbenchToasts(): { toasts: Toast[]; toast: ToastFn } {
  const [toasts, setToasts] = useState<Toast[]>([]);
  const toastIdRef = useRef(0);
  const timerIdsRef = useRef<number[]>([]);

  useEffect(() => {
    return () => {
      timerIdsRef.current.forEach((timerId) => window.clearTimeout(timerId));
      timerIdsRef.current = [];
    };
  }, []);

  const toast = useCallback<ToastFn>((kind, title, detail) => {
    const id = ++toastIdRef.current;
    setToasts((current) => [...current.slice(-3), { id, kind, title, detail }]);
    const timerId = window.setTimeout(() => {
      setToasts((current) => current.filter((item) => item.id !== id));
      timerIdsRef.current = timerIdsRef.current.filter((item) => item !== timerId);
    }, 3600);
    timerIdsRef.current.push(timerId);
  }, []);

  return { toasts, toast };
}
