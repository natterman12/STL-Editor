// Feature evaluators that live in their own file. Each module calls register() on import.
export const EVALUATORS = {};
export const LABELS = {};
export function register(key, fn, label) {
  EVALUATORS[key] = fn;
  LABELS[key] = label;
}
