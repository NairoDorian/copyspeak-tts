import { vi } from "vitest";

export const goto = vi.fn().mockResolvedValue(true);
export const invalidate = vi.fn().mockResolvedValue(true);
export const invalidateAll = vi.fn().mockResolvedValue(true);
export const afterNavigate = vi.fn();
export const beforeNavigate = vi.fn();
