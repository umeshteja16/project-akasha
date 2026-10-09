// Server status for Settings → System (`GET /api/v1/system/status`).

import { queryOptions } from "@tanstack/react-query";
import { type Api, type Schemas, unwrap } from "./client";

export type SystemStatus = Schemas["SystemStatus"];
export type ComponentStatus = Schemas["ComponentStatus"];
export type WorkerHealth = Schemas["WorkerHealth"];

/** Polled while the panel is open: queue numbers move. */
export const systemStatusQuery = (api: Api) =>
  queryOptions({
    queryKey: ["system", "status"] as const,
    queryFn: ({ signal }) => unwrap(api.GET("/api/v1/system/status", { signal })),
    refetchInterval: 10_000,
    staleTime: 5_000,
  });
