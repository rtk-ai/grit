import { query, transaction } from "./db";
import { generateId, validateEmail, slugify, formatDate } from "./utils";
import { hashPassword, validateSession } from "./auth";

interface User {
  id: string;
  email: string;
  name: string;
  slug: string;
  role: "admin" | "user" | "viewer";
  createdAt: string;
  updatedAt: string;
}

interface CreateUserInput {
  email: string;
  name: string;
  password: string;
  role?: "admin" | "user" | "viewer";
}

interface UpdateUserInput {
  name?: string;
  email?: string;
  role?: "admin" | "user" | "viewer";
}

interface PaginatedResult<T> {
  data: T[];
  total: number;
  page: number;
  pageSize: number;
  hasMore: boolean;
}

export async function getUser(userId: string, requesterId: string): Promise<User | null> {
  const session = await validateSession(requesterId);
  if (!session.valid) {
    throw new Error("Unauthorized: invalid session");
  }

  const result = await query<User>(
    "SELECT id, email, name, slug, role, created_at, updated_at FROM users WHERE id = $1",
    [userId]
  );

  if (result.rowCount === 0) {
    return null;
  }

  const user = result.rows[0];
  console.log(`[api:getUser] Retrieved user ${user.id} (${user.email})`);
  return user;
}

export async function createUser(input: CreateUserInput): Promise<User> {
  const { valid, reason } = validateEmail(input.email);
  if (!valid) {
    throw new Error(`Invalid email: ${reason}`);
  }

  if (!input.name || input.name.trim().length < 2) {
    throw new Error("Name must be at least 2 characters");
  }

  // Check for existing user
  const existing = await query(
    "SELECT id FROM users WHERE email = $1",
    [input.email.toLowerCase().trim()]
  );

  if (existing.rowCount > 0) {
    throw new Error(`User with email ${input.email} already exists`);
  }

  const passwordHash = await hashPassword(input.password);
  const now = formatDate(new Date());
  const user: User = {
    id: generateId("usr"),
    email: input.email.toLowerCase().trim(),
    name: input.name.trim(),
    slug: slugify(input.name),
    role: input.role ?? "user",
    createdAt: now,
    updatedAt: now,
  };

  await query(
    `INSERT INTO users (id, email, name, slug, role, password_hash, created_at, updated_at)
     VALUES ($1, $2, $3, $4, $5, $6, $7, $8)`,
    [user.id, user.email, user.name, user.slug, user.role, passwordHash, user.createdAt, user.updatedAt]
  );

  console.log(`[api:createUser] Created user ${user.id} (${user.email})`);
  return user;
}

export async function updateUser(
  userId: string,
  updates: UpdateUserInput,
  requesterId: string
): Promise<User> {
  const session = await validateSession(requesterId);
  if (!session.valid) {
    throw new Error("Unauthorized: invalid session");
  }

  const current = await getUser(userId, requesterId);
  if (!current) {
    throw new Error(`User ${userId} not found`);
  }

  if (updates.email) {
    const { valid, reason } = validateEmail(updates.email);
    if (!valid) {
      throw new Error(`Invalid email: ${reason}`);
    }
  }

  const setClauses: string[] = [];
  const params: unknown[] = [];
  let paramIndex = 1;

  if (updates.name) {
    setClauses.push(`name = $${paramIndex++}`);
    params.push(updates.name.trim());
  }
  if (updates.email) {
    setClauses.push(`email = $${paramIndex++}`);
    params.push(updates.email.toLowerCase().trim());
  }
  if (updates.role) {
    setClauses.push(`role = $${paramIndex++}`);
    params.push(updates.role);
  }

  setClauses.push(`updated_at = $${paramIndex++}`);
  params.push(formatDate(new Date()));
  params.push(userId);

  await query(
    `UPDATE users SET ${setClauses.join(", ")} WHERE id = $${paramIndex}`,
    params
  );

  const updated = await getUser(userId, requesterId);
  console.log(`[api:updateUser] Updated user ${userId}: ${setClauses.length - 1} fields changed`);
  return updated!;
}

export async function deleteUser(userId: string, requesterId: string): Promise<{ deleted: boolean }> {
  const session = await validateSession(requesterId);
  if (!session.valid) {
    throw new Error("Unauthorized: invalid session");
  }

  // Use a transaction to clean up sessions and the user atomically
  await transaction([
    { sql: "DELETE FROM sessions WHERE user_id = $1", params: [userId] },
    { sql: "DELETE FROM users WHERE id = $1", params: [userId] },
  ]);

  console.log(`[api:deleteUser] Deleted user ${userId} and associated sessions`);
  return { deleted: true };
}

export async function listUsers(
  requesterId: string,
  options: { page?: number; pageSize?: number; role?: string; search?: string } = {}
): Promise<PaginatedResult<Omit<User, "slug">>> {
  const session = await validateSession(requesterId);
  if (!session.valid) {
    throw new Error("Unauthorized: invalid session");
  }

  const page = Math.max(1, options.page ?? 1);
  const pageSize = Math.min(100, Math.max(1, options.pageSize ?? 20));
  const offset = (page - 1) * pageSize;

  const conditions: string[] = [];
  const params: unknown[] = [];
  let paramIndex = 1;

  if (options.role) {
    conditions.push(`role = $${paramIndex++}`);
    params.push(options.role);
  }

  if (options.search) {
    conditions.push(`(name ILIKE $${paramIndex} OR email ILIKE $${paramIndex})`);
    params.push(`%${options.search}%`);
    paramIndex++;
  }

  const whereClause = conditions.length > 0 ? `WHERE ${conditions.join(" AND ")}` : "";

  const countResult = await query<{ count: number }>(
    `SELECT COUNT(*) as count FROM users ${whereClause}`,
    params
  );

  const total = countResult.rows[0]?.count ?? 0;

  params.push(pageSize, offset);
  const result = await query<User>(
    `SELECT id, email, name, role, created_at, updated_at FROM users ${whereClause} ORDER BY created_at DESC LIMIT $${paramIndex++} OFFSET $${paramIndex}`,
    params
  );

  console.log(`[api:listUsers] Returned ${result.rowCount} of ${total} users (page ${page})`);

  return {
    data: result.rows,
    total,
    page,
    pageSize,
    hasMore: offset + pageSize < total,
  };
}
