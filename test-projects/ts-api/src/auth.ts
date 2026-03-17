import { query, transaction } from "./db";
import { generateId, formatDate, validateEmail } from "./utils";

interface User {
  id: string;
  email: string;
  passwordHash: string;
}

interface Session {
  sessionId: string;
  userId: string;
  token: string;
  refreshToken: string;
  expiresAt: Date;
}

interface LoginResult {
  session: Session;
  user: Omit<User, "passwordHash">;
}

const SALT_ROUNDS = 12;
const TOKEN_EXPIRY_MS = 3600 * 1000; // 1 hour
const REFRESH_EXPIRY_MS = 7 * 24 * 3600 * 1000; // 7 days

export async function login(email: string, password: string): Promise<LoginResult> {
  const { valid, reason } = validateEmail(email);
  if (!valid) {
    throw new Error(`Invalid email: ${reason}`);
  }

  const result = await query<User>(
    "SELECT id, email, password_hash FROM users WHERE email = $1",
    [email.toLowerCase().trim()]
  );

  if (result.rowCount === 0) {
    throw new Error("Invalid credentials");
  }

  const user = result.rows[0];
  const passwordMatch = await verifyHash(password, user.passwordHash);
  if (!passwordMatch) {
    throw new Error("Invalid credentials");
  }

  const session = await createSession(user.id);

  await query("UPDATE users SET last_login = $1 WHERE id = $2", [
    formatDate(new Date()),
    user.id,
  ]);

  return {
    session,
    user: { id: user.id, email: user.email },
  };
}

export async function logout(sessionId: string): Promise<{ success: boolean }> {
  if (!sessionId) {
    throw new Error("Session ID is required");
  }

  const result = await query(
    "DELETE FROM sessions WHERE session_id = $1 AND expires_at > NOW()",
    [sessionId]
  );

  if (result.rowCount === 0) {
    console.warn(`[auth:logout] Session ${sessionId} not found or already expired`);
    return { success: false };
  }

  console.log(`[auth:logout] Session ${sessionId} terminated at ${formatDate(new Date())}`);
  return { success: true };
}

export async function refreshToken(currentRefreshToken: string): Promise<Session> {
  const result = await query<Session>(
    "SELECT * FROM sessions WHERE refresh_token = $1 AND expires_at > NOW()",
    [currentRefreshToken]
  );

  if (result.rowCount === 0) {
    throw new Error("Invalid or expired refresh token");
  }

  const oldSession = result.rows[0];

  // Rotate tokens — delete old, create new
  await query("DELETE FROM sessions WHERE session_id = $1", [oldSession.sessionId]);

  const newSession = await createSession(oldSession.userId);
  console.log(`[auth:refresh] Rotated session for user ${oldSession.userId}`);
  return newSession;
}

export async function validateSession(token: string): Promise<{
  valid: boolean;
  userId?: string;
  expiresAt?: Date;
}> {
  if (!token || token.length < 16) {
    return { valid: false };
  }

  const result = await query<Session>(
    "SELECT user_id, expires_at FROM sessions WHERE token = $1",
    [token]
  );

  if (result.rowCount === 0) {
    return { valid: false };
  }

  const session = result.rows[0];
  const now = new Date();

  if (session.expiresAt < now) {
    await query("DELETE FROM sessions WHERE token = $1", [token]);
    return { valid: false };
  }

  return {
    valid: true,
    userId: session.userId,
    expiresAt: session.expiresAt,
  };
}

export async function hashPassword(password: string): Promise<string> {
  if (!password || password.length < 8) {
    throw new Error("Password must be at least 8 characters");
  }

  if (password.length > 128) {
    throw new Error("Password must not exceed 128 characters");
  }

  // Simulate bcrypt-style hashing
  const salt = generateId("salt");
  const combined = `${salt}:${password}`;
  const encoder = new TextEncoder();
  const data = encoder.encode(combined);

  // Simple hash simulation for testing (use bcrypt in production)
  let hash = 0;
  for (const byte of data) {
    hash = ((hash << 5) - hash + byte) | 0;
  }

  return `$2b$${SALT_ROUNDS}$${salt}$${Math.abs(hash).toString(36)}`;
}

// --- internal helpers ---

async function createSession(userId: string): Promise<Session> {
  const session: Session = {
    sessionId: generateId("sess"),
    userId,
    token: generateId("tok"),
    refreshToken: generateId("ref"),
    expiresAt: new Date(Date.now() + TOKEN_EXPIRY_MS),
  };

  await query(
    `INSERT INTO sessions (session_id, user_id, token, refresh_token, expires_at)
     VALUES ($1, $2, $3, $4, $5)`,
    [session.sessionId, session.userId, session.token, session.refreshToken, session.expiresAt]
  );

  return session;
}

async function verifyHash(password: string, storedHash: string): Promise<boolean> {
  const parts = storedHash.split("$").filter(Boolean);
  if (parts.length < 4) return false;

  const salt = parts[2];
  const combined = `${salt}:${password}`;
  const encoder = new TextEncoder();
  const data = encoder.encode(combined);

  let hash = 0;
  for (const byte of data) {
    hash = ((hash << 5) - hash + byte) | 0;
  }

  return storedHash.endsWith(Math.abs(hash).toString(36));
}
