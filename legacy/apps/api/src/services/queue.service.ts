import { Queue } from "bullmq";
import { Redis } from "ioredis";
import { validateEnv } from "@akasha/shared";
import * as dotenv from "dotenv";

dotenv.config();
const env = validateEnv(process.env);

// Initialize a single shared Redis connection specifically for BullMQ
// Critical constraint: maxRetriesPerRequest must be null for BullMQ to avoid connection issues.
export const queueRedisConnection = new Redis(env.REDIS_URL, {
  maxRetriesPerRequest: null,
});

export const extractionQueue = new Queue("file-extraction", {
  connection: queueRedisConnection,
  defaultJobOptions: {
    attempts: 3,
    backoff: {
      type: "exponential",
      delay: 30000, // 30s → 60s → 120s exponential backoff
    },
    removeOnComplete: true, // Auto-prune successfully completed tasks
    removeOnFail: false,    // Maintain failures in Redis for diagnostic analysis
  },
});
