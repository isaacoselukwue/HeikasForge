export type ClassValue = string | false | null | undefined;

export function cx(...values: ClassValue[]): string {
  return values
    .filter((value): value is string => typeof value === "string" && value.length > 0)
    .join(" ");
}
