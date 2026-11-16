import { create } from "zustand";

export type Toast = { id: number; tone: "error" | "info" | "success"; title: string; body?: string };

type ToastState = { toasts: Toast[]; dismiss: (id: number) => void };

let nextId = 1;

export const useToasts = create<ToastState>((set) => ({
  toasts: [],
  dismiss: (id) => set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })),
}));

function push(tone: Toast["tone"], title: string, body?: string) {
  const id = nextId++;
  useToasts.setState((s) => ({ toasts: [...s.toasts.slice(-4), { id, tone, title, body }] }));
  setTimeout(() => useToasts.getState().dismiss(id), tone === "error" ? 8000 : 3500);
}

export const toast = {
  error: (title: string, body?: string) => push("error", title, body),
  info: (title: string, body?: string) => push("info", title, body),
  success: (title: string, body?: string) => push("success", title, body),
};
