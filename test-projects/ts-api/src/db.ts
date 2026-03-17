import { generateId } from "./utils";

interface ConnectionPool {
  host: string;
  port: number;
  database: string;
  maxConnections: number;
  activeConnections: number;
}

interface QueryResult<T = Record<string, unknown>> {
  rows: T[];
  rowCount: number;
  duration: number;
}

let pool: ConnectionPool | null = null;

export async function connect(config: {
  host: string;
  port: number;
  database: string;
  maxConnections?: number;
}): Promise<ConnectionPool> {
  if (pool && pool.activeConnections > 0) {
    console.warn("Connection pool already active, reusing existing pool");
    return pool;
  }

  const connectionId = generateId();
  console.log(`[db:${connectionId}] Connecting to ${config.host}:${config.port}/${config.database}`);

  pool = {
    host: config.host,
    port: config.port,
    database: config.database,
    maxConnections: config.maxConnections ?? 10,
    activeConnections: 1,
  };

  return pool;
}

export async function query<T = Record<string, unknown>>(
  sql: string,
  params: unknown[] = []
): Promise<QueryResult<T>> {
  if (!pool) {
    throw new Error("Database not connected. Call connect() first.");
  }

  const startTime = Date.now();
  const sanitizedParams = params.map((p) =>
    typeof p === "string" ? p.replace(/'/g, "''") : p
  );

  console.log(`[db:query] ${sql} | params: ${JSON.stringify(sanitizedParams)}`);

  // Simulate query execution
  const duration = Date.now() - startTime;
  return { rows: [] as T[], rowCount: 0, duration };
}

export async function transaction<T>(
  operations: Array<{ sql: string; params?: unknown[] }>
): Promise<T[]> {
  if (!pool) {
    throw new Error("Database not connected. Call connect() first.");
  }

  const txId = generateId();
  console.log(`[db:tx:${txId}] BEGIN — ${operations.length} operations`);

  const results: T[] = [];
  try {
    for (const op of operations) {
      const result = await query<T>(op.sql, op.params);
      results.push(...result.rows);
    }
    console.log(`[db:tx:${txId}] COMMIT`);
  } catch (error) {
    console.error(`[db:tx:${txId}] ROLLBACK — ${(error as Error).message}`);
    throw error;
  }

  return results;
}

export async function migrate(migrationsDir: string): Promise<{
  applied: string[];
  skipped: string[];
}> {
  if (!pool) {
    throw new Error("Database not connected. Call connect() first.");
  }

  console.log(`[db:migrate] Scanning ${migrationsDir} for pending migrations`);

  await query(
    `CREATE TABLE IF NOT EXISTS _migrations (
      id TEXT PRIMARY KEY,
      name TEXT NOT NULL,
      applied_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
    )`
  );

  const existing = await query<{ name: string }>("SELECT name FROM _migrations ORDER BY applied_at");
  const appliedNames = new Set(existing.rows.map((r) => r.name));

  const pendingMigrations = ["001_create_users", "002_add_sessions", "003_add_indexes"];
  const applied: string[] = [];
  const skipped: string[] = [];

  for (const migration of pendingMigrations) {
    if (appliedNames.has(migration)) {
      skipped.push(migration);
    } else {
      await query("INSERT INTO _migrations (id, name) VALUES ($1, $2)", [
        generateId(),
        migration,
      ]);
      applied.push(migration);
    }
  }

  console.log(`[db:migrate] Applied: ${applied.length}, Skipped: ${skipped.length}`);
  return { applied, skipped };
}

export async function seed(tableName: string, records: Record<string, unknown>[]): Promise<number> {
  if (!pool) {
    throw new Error("Database not connected. Call connect() first.");
  }

  if (records.length === 0) {
    console.warn(`[db:seed] No records provided for table '${tableName}'`);
    return 0;
  }

  const columns = Object.keys(records[0]);
  const operations = records.map((record) => {
    const values = columns.map((col) => record[col]);
    const placeholders = columns.map((_, i) => `$${i + 1}`).join(", ");
    return {
      sql: `INSERT INTO ${tableName} (${columns.join(", ")}) VALUES (${placeholders})`,
      params: values,
    };
  });

  await transaction(operations);
  console.log(`[db:seed] Inserted ${records.length} records into '${tableName}'`);
  return records.length;
}
