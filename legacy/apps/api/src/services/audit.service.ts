import { db } from "../db.js";
import { auditLog } from "@akasha/db";

export interface AuditLogOptions {
  userId: string | null;
  action: string;
  fileId?: string;
  ipAddress?: string;
}

/**
 * Log a user action to the secure audit_log table.
 * Designed to fail gracefully to avoid blocking the primary user flows.
 */
export async function logActivity(options: AuditLogOptions): Promise<void> {
  try {
    await db.insert(auditLog).values({
      userId: options.userId,
      action: options.action,
      fileId: options.fileId || null,
      ipAddress: options.ipAddress || null,
    });
  } catch (err) {
    // Graceful failure so auditing doesn't interrupt core system operations
    console.error(`[AuditService] Failed to write to audit_log table:`, err);
  }
}
