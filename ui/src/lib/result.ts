import type { AppError, ErrorKind } from "@/bindings";

/** What every `typedError<…>` command in bindings.ts resolves to. */
export type CommandResult<T> = { status: "ok"; data: T } | { status: "error"; error: AppError };

const ERROR_KINDS: ReadonlySet<string> = new Set<ErrorKind>([
  "device",
  "deviceInUse",
  "notRunning",
  "latency",
  "unsupported",
  "config",
  "update",
  "internal",
]);

/** A failed command, thrown so TanStack Query treats it as an error. */
export class AppErrorException extends Error {
  readonly kind: ErrorKind;

  constructor(error: AppError) {
    super(error.message);
    this.name = "AppError";
    this.kind = error.kind;
  }
}

export function isAppError(value: unknown): value is AppError {
  if (typeof value !== "object" || value === null) return false;
  const kind: unknown = Reflect.get(value, "kind");
  const message: unknown = Reflect.get(value, "message");
  return typeof kind === "string" && ERROR_KINDS.has(kind) && typeof message === "string";
}

/** Resolve a typedError result to its data, or throw its `AppError`. */
export async function unwrap<T>(pending: Promise<CommandResult<T>>): Promise<T> {
  const result = await pending;
  if (result.status === "ok") return result.data;
  throw new AppErrorException(result.error);
}

/** Best-effort user-facing message for anything thrown by a command. */
export function errorMessage(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  if (isAppError(error)) return error.message;
  if (typeof error === "object" && error !== null) {
    const message: unknown = Reflect.get(error, "message");
    if (typeof message === "string") return message;
  }
  return "Something went wrong.";
}
