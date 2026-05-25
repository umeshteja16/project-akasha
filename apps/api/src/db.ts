import { getDb } from "@akasha/db";
import { validateEnv } from "@akasha/shared";
import * as dotenv from "dotenv";

dotenv.config();
const env = validateEnv(process.env);

export const db: ReturnType<typeof getDb> = getDb(env.DATABASE_URL);
