import { drizzle } from "drizzle-orm/postgres-js";
import postgres from "postgres";
import * as schema from "./schema/index.js";

export * from "./schema/index.js";
export { eq, and, or, sql, desc, asc } from "drizzle-orm";

let queryClient: postgres.Sql | null = null;

export function getDb(databaseUrl: string) {
  if (!queryClient) {
    queryClient = postgres(databaseUrl, { max: 10 });
  }
  return drizzle(queryClient, { schema });
}
