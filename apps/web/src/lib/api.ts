import { useAuthStore } from "../store/auth.store";

// Queue for holding requests while a token refresh is in progress
let isRefreshing = false;
let refreshQueue: Array<(token: string) => void> = [];

function processQueue(_error: Error | null, token: string | null) {
  refreshQueue.forEach((callback) => {
    if (token) {
      callback(token);
    }
  });
  refreshQueue = [];
}

interface RequestOptions extends RequestInit {
  bodyData?: any;
  isMultipart?: boolean;
}

export class ApiError extends Error {
  status: number;
  code?: string;

  constructor(message: string, status: number, code?: string) {
    super(message);
    this.name = "ApiError";
    this.status = status;
    this.code = code;
  }
}

async function request(path: string, options: RequestOptions = {}): Promise<any> {
  const { accessToken } = useAuthStore.getState();
  const headers = new Headers(options.headers || {});

  // 1. Inject Authorization header if JWT exists in store
  if (accessToken && !headers.has("Authorization")) {
    headers.set("Authorization", `Bearer ${accessToken}`);
  }

  // 2. Set default content type to JSON unless it's a multipart file upload
  if (!options.isMultipart && !headers.has("Content-Type") && options.bodyData) {
    headers.set("Content-Type", "application/json");
  }

  const fetchOptions: RequestInit = {
    ...options,
    headers,
  };

  // Convert bodyData to string if it's a JSON payload
  if (options.bodyData) {
    fetchOptions.body = options.isMultipart ? options.bodyData : JSON.stringify(options.bodyData);
  }

  try {
    const response = await fetch(path, fetchOptions);

    // Handle 401 Unauthorized (JWT expired, need silent refresh)
    if (response.status === 401) {
      // Don't attempt silent refresh for auth routes themselves
      if (path.includes("/api/v1/auth/login") || path.includes("/api/v1/auth/refresh")) {
        const errorData = await response.json().catch(() => ({}));
        throw new ApiError(
          errorData?.error?.message || "Authentication failed",
          response.status,
          errorData?.error?.code
        );
      }

      // If already refreshing, queue this request
      if (isRefreshing) {
        return new Promise((resolve) => {
          refreshQueue.push((newToken: string) => {
            headers.set("Authorization", `Bearer ${newToken}`);
            resolve(request(path, options));
          });
        });
      }

      isRefreshing = true;

      try {
        // Trigger silent refresh (Cookie-based rotation)
        const refreshResponse = await fetch("/api/v1/auth/refresh", {
          method: "POST",
        });

        if (!refreshResponse.ok) {
          throw new Error("Refresh failed");
        }

        const refreshData = await refreshResponse.json();
        const newAccessToken = refreshData.accessToken || refreshData.data?.accessToken;

        if (!newAccessToken) {
          throw new Error("Token missing in refresh response");
        }

        // Update auth state in Zustand
        useAuthStore.getState().setAuth(newAccessToken);

        // Process any queued requests with the new token
        processQueue(null, newAccessToken);
        isRefreshing = false;

        // Retry the original request
        headers.set("Authorization", `Bearer ${newAccessToken}`);
        return request(path, options);
      } catch (refreshErr) {
        processQueue(new Error("Session expired"), null);
        isRefreshing = false;
        useAuthStore.getState().clearAuth();
        throw new ApiError("Your session has expired. Please log in again.", 401, "UNAUTHORIZED");
      }
    }

    // Handle standard API success or other HTTP errors
    const data = await response.json().catch(() => null);

    if (!response.ok) {
      throw new ApiError(
        data?.error?.message || "An unexpected error occurred",
        response.status,
        data?.error?.code
      );
    }

    return data;
  } catch (err) {
    if (err instanceof ApiError) {
      throw err;
    }
    throw new ApiError((err as Error).message || "Network connection error", 500);
  }
}

export function uploadWithProgress(
  path: string,
  formData: FormData,
  onProgress: (percent: number) => void
): Promise<any> {
  return new Promise((resolve, reject) => {
    const xhr = new XMLHttpRequest();

    xhr.upload.addEventListener("progress", (event) => {
      if (event.lengthComputable) {
        const percent = Math.round((event.loaded / event.total) * 100);
        onProgress(percent);
      }
    });

    xhr.addEventListener("load", async () => {
      const responseText = xhr.responseText;
      let data: any = null;
      try {
        data = JSON.parse(responseText);
      } catch {
        // Not JSON
      }

      if (xhr.status === 401) {
        // Silently refresh token and retry upload
        if (isRefreshing) {
          refreshQueue.push((_newToken: string) => {
            uploadWithProgress(path, formData, onProgress)
              .then(resolve)
              .catch(reject);
          });
          return;
        }

        isRefreshing = true;
        try {
          const refreshResponse = await fetch("/api/v1/auth/refresh", {
            method: "POST",
          });

          if (!refreshResponse.ok) {
            throw new Error("Refresh failed");
          }

          const refreshData = await refreshResponse.json();
          const newAccessToken = refreshData.accessToken || refreshData.data?.accessToken;

          if (!newAccessToken) {
            throw new Error("Token missing in refresh response");
          }

          useAuthStore.getState().setAuth(newAccessToken);
          processQueue(null, newAccessToken);
          isRefreshing = false;

          // Retry the upload
          uploadWithProgress(path, formData, onProgress)
            .then(resolve)
            .catch(reject);
        } catch (refreshErr) {
          processQueue(new Error("Session expired"), null);
          isRefreshing = false;
          useAuthStore.getState().clearAuth();
          reject(new ApiError("Your session has expired. Please log in again.", 401, "UNAUTHORIZED"));
        }
        return;
      }

      if (xhr.status >= 200 && xhr.status < 300) {
        resolve(data);
      } else {
        reject(
          new ApiError(
            data?.error?.message || "An unexpected error occurred",
            xhr.status,
            data?.error?.code
          )
        );
      }
    });

    xhr.addEventListener("error", () => {
      reject(new ApiError("Network connection error", 500));
    });

    xhr.open("POST", path);

    const { accessToken } = useAuthStore.getState();
    if (accessToken) {
      xhr.setRequestHeader("Authorization", `Bearer ${accessToken}`);
    }

    xhr.send(formData);
  });
}

export const api = {
  get: (path: string, options?: RequestInit) => request(path, { ...options, method: "GET" }),
  post: (path: string, body?: any, options?: RequestInit) =>
    request(path, { ...options, method: "POST", bodyData: body }),
  put: (path: string, body?: any, options?: RequestInit) =>
    request(path, { ...options, method: "PUT", bodyData: body }),
  patch: (path: string, body?: any, options?: RequestInit) =>
    request(path, { ...options, method: "PATCH", bodyData: body }),
  delete: (path: string, options?: RequestInit) => request(path, { ...options, method: "DELETE" }),
  upload: (path: string, formData: FormData, options?: RequestInit) =>
    request(path, {
      ...options,
      method: "POST",
      bodyData: formData,
      isMultipart: true,
    }),
  uploadWithProgress,
};

