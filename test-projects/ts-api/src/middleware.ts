import { validateSession } from "./auth";
import { generateId, formatDate, parseJSON } from "./utils";

interface Request {
  method: string;
  path: string;
  headers: Record<string, string>;
  body?: string;
  ip?: string;
}

interface Response {
  status: (code: number) => Response;
  json: (data: unknown) => void;
  set: (header: string, value: string) => Response;
  headersSent: boolean;
}

type NextFunction = (error?: Error) => void;
type Middleware = (req: Request, res: Response, next: NextFunction) => void | Promise<void>;

interface RateLimitStore {
  [ip: string]: { count: number; resetAt: number };
}

const rateLimitStore: RateLimitStore = {};

export function cors(options: {
  origins?: string[];
  methods?: string[];
  maxAge?: number;
} = {}): Middleware {
  const allowedOrigins = options.origins ?? ["*"];
  const allowedMethods = options.methods ?? ["GET", "POST", "PUT", "DELETE", "OPTIONS"];
  const maxAge = options.maxAge ?? 86400;

  return (req: Request, res: Response, next: NextFunction) => {
    const origin = req.headers["origin"] ?? "*";
    const isAllowed = allowedOrigins.includes("*") || allowedOrigins.includes(origin);

    if (!isAllowed) {
      res.status(403).json({ error: "Origin not allowed" });
      return;
    }

    res.set("Access-Control-Allow-Origin", isAllowed ? origin : "");
    res.set("Access-Control-Allow-Methods", allowedMethods.join(", "));
    res.set("Access-Control-Allow-Headers", "Content-Type, Authorization");
    res.set("Access-Control-Max-Age", String(maxAge));

    if (req.method === "OPTIONS") {
      res.status(204).json({});
      return;
    }

    next();
  };
}

export function rateLimit(options: {
  windowMs?: number;
  maxRequests?: number;
  message?: string;
} = {}): Middleware {
  const windowMs = options.windowMs ?? 60_000;
  const maxRequests = options.maxRequests ?? 100;
  const message = options.message ?? "Too many requests, please try again later";

  return (req: Request, res: Response, next: NextFunction) => {
    const clientIp = req.ip ?? req.headers["x-forwarded-for"] ?? "unknown";
    const now = Date.now();

    if (!rateLimitStore[clientIp] || rateLimitStore[clientIp].resetAt < now) {
      rateLimitStore[clientIp] = { count: 0, resetAt: now + windowMs };
    }

    rateLimitStore[clientIp].count++;

    const remaining = Math.max(0, maxRequests - rateLimitStore[clientIp].count);
    res.set("X-RateLimit-Limit", String(maxRequests));
    res.set("X-RateLimit-Remaining", String(remaining));
    res.set("X-RateLimit-Reset", String(rateLimitStore[clientIp].resetAt));

    if (rateLimitStore[clientIp].count > maxRequests) {
      res.status(429).json({ error: message, retryAfter: Math.ceil((rateLimitStore[clientIp].resetAt - now) / 1000) });
      return;
    }

    next();
  };
}

export function authenticate(options: {
  optional?: boolean;
  roles?: string[];
} = {}): Middleware {
  return async (req: Request, res: Response, next: NextFunction) => {
    const authHeader = req.headers["authorization"];

    if (!authHeader) {
      if (options.optional) {
        next();
        return;
      }
      res.status(401).json({ error: "Authorization header required" });
      return;
    }

    const [scheme, token] = authHeader.split(" ");
    if (scheme !== "Bearer" || !token) {
      res.status(401).json({ error: "Invalid authorization format. Expected: Bearer <token>" });
      return;
    }

    const result = await validateSession(token);
    if (!result.valid) {
      res.status(401).json({ error: "Invalid or expired token" });
      return;
    }

    // Attach user context to request
    (req as any).userId = result.userId;
    (req as any).sessionExpiresAt = result.expiresAt;

    next();
  };
}

export function logRequest(options: {
  includeBody?: boolean;
  slowThresholdMs?: number;
} = {}): Middleware {
  const slowThreshold = options.slowThresholdMs ?? 1000;

  return (req: Request, res: Response, next: NextFunction) => {
    const requestId = generateId("req");
    const startTime = Date.now();
    const timestamp = formatDate(new Date());

    console.log(
      `[${timestamp}] ${requestId} --> ${req.method} ${req.path} from ${req.ip ?? "unknown"}`
    );

    if (options.includeBody && req.body) {
      try {
        const parsed = parseJSON(req.body);
        console.log(`[${requestId}] Body: ${JSON.stringify(parsed).substring(0, 500)}`);
      } catch {
        console.log(`[${requestId}] Body: (unparseable)`);
      }
    }

    // Wrap next to log completion
    const originalNext = next;
    const wrappedNext: NextFunction = (error?: Error) => {
      const duration = Date.now() - startTime;
      const slow = duration > slowThreshold ? " [SLOW]" : "";

      if (error) {
        console.error(`[${timestamp}] ${requestId} <-- ERROR ${duration}ms${slow}: ${error.message}`);
      } else {
        console.log(`[${timestamp}] ${requestId} <-- ${duration}ms${slow}`);
      }

      originalNext(error);
    };

    next = wrappedNext;
    next();
  };
}

export function errorHandler(): Middleware {
  return (req: Request, res: Response, next: NextFunction) => {
    try {
      next();
    } catch (error) {
      const err = error as Error & { statusCode?: number; code?: string };
      const statusCode = err.statusCode ?? 500;
      const requestId = generateId("err");

      console.error(`[errorHandler:${requestId}] ${err.code ?? "UNKNOWN"}: ${err.message}`);

      if (statusCode >= 500) {
        console.error(`[errorHandler:${requestId}] Stack: ${err.stack}`);
      }

      if (!res.headersSent) {
        const responseBody: Record<string, unknown> = {
          error: statusCode >= 500 ? "Internal server error" : err.message,
          requestId,
          timestamp: formatDate(new Date()),
        };

        if (statusCode < 500 && err.code) {
          responseBody.code = err.code;
        }

        res.status(statusCode).json(responseBody);
      }
    }
  };
}
