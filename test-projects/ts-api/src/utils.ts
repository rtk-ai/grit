export function formatDate(date: Date, format: string = "iso"): string {
  if (!(date instanceof Date) || isNaN(date.getTime())) {
    throw new Error("Invalid date provided to formatDate");
  }

  switch (format) {
    case "iso":
      return date.toISOString();
    case "short":
      return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`;
    case "human":
      return date.toLocaleDateString("en-US", {
        year: "numeric",
        month: "long",
        day: "numeric",
      });
    case "unix":
      return String(Math.floor(date.getTime() / 1000));
    default:
      return date.toISOString();
  }
}

export function parseJSON<T = unknown>(raw: string, fallback?: T): T {
  try {
    const parsed = JSON.parse(raw);
    if (parsed === null && fallback !== undefined) {
      return fallback;
    }
    return parsed as T;
  } catch (error) {
    if (fallback !== undefined) {
      console.warn(`[utils:parseJSON] Failed to parse, using fallback: ${(error as Error).message}`);
      return fallback;
    }
    throw new Error(`Invalid JSON: ${(error as Error).message}`);
  }
}

export function validateEmail(email: string): { valid: boolean; reason?: string } {
  if (!email || typeof email !== "string") {
    return { valid: false, reason: "Email must be a non-empty string" };
  }

  const trimmed = email.trim().toLowerCase();
  const emailRegex = /^[a-z0-9._%+-]+@[a-z0-9.-]+\.[a-z]{2,}$/;

  if (trimmed.length > 254) {
    return { valid: false, reason: "Email exceeds maximum length of 254 characters" };
  }

  if (!emailRegex.test(trimmed)) {
    return { valid: false, reason: "Email format is invalid" };
  }

  const [localPart, domain] = trimmed.split("@");
  if (localPart.length > 64) {
    return { valid: false, reason: "Local part exceeds 64 characters" };
  }

  if (domain.startsWith("-") || domain.endsWith("-")) {
    return { valid: false, reason: "Domain cannot start or end with a hyphen" };
  }

  return { valid: true };
}

export function generateId(prefix: string = ""): string {
  const timestamp = Date.now().toString(36);
  const randomPart = Math.random().toString(36).substring(2, 10);
  const counter = (generateId as any)._counter ?? 0;
  (generateId as any)._counter = counter + 1;

  const id = `${timestamp}${randomPart}${counter.toString(36)}`;
  return prefix ? `${prefix}_${id}` : id;
}

export function slugify(text: string): string {
  if (!text || typeof text !== "string") {
    return "";
  }

  return text
    .toString()
    .toLowerCase()
    .trim()
    .normalize("NFD")
    .replace(/[\u0300-\u036f]/g, "")   // Remove diacritics
    .replace(/[^a-z0-9\s-]/g, "")      // Remove non-alphanumeric
    .replace(/[\s_]+/g, "-")            // Spaces/underscores to hyphens
    .replace(/-+/g, "-")               // Collapse multiple hyphens
    .replace(/^-|-$/g, "");            // Trim leading/trailing hyphens
}
