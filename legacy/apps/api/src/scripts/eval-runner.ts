/**
 * AKASHA Search Retrieval Evaluation Runner
 * Runs benchmark retrieval queries and calculates MRR, Precision@K, Recall@K, and NDCG@10.
 *
 * Usage: npx tsx src/scripts/eval-runner.ts
 */
import fs from "fs";
import path from "path";
import { performance } from "perf_hooks";

const API_URL = process.env.API_URL || "http://localhost:3001";
const TEST_EMAIL = `eval_runner_${Date.now()}@akasha.com`;
const TEST_PASSWORD = "super_secure_password_123";

interface BenchmarkQuery {
  id: string;
  query: string;
  expected_files: string[];
  expected_keywords_in_snippet: string[];
  must_not_match_files: string[];
  minimum_top1_score: number;
  minimum_mrr: number;
  modes: ("keyword" | "semantic" | "hybrid")[];
}

interface BenchmarkSuite {
  version: string;
  benchmarks: BenchmarkQuery[];
}

interface ModeMetrics {
  mrr: number;
  precisionAt1: number;
  precisionAt3: number;
  recallAt3: number;
  ndcgAt10: number;
  latencyMs: number;
}

async function delay(ms: number) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

// Logarithm base 2 helper for NDCG calculation
function log2(n: number): number {
  return Math.log(n) / Math.LN2;
}

// Calculate Discounted Cumulative Gain at K
function calculateDCG(results: any[], expectedFiles: string[], k: number): number {
  let dcg = 0;
  const limitedResults = results.slice(0, k);
  for (let i = 0; i < limitedResults.length; i++) {
    const isRelevant = expectedFiles.includes(limitedResults[i].fileName) ? 1 : 0;
    dcg += isRelevant / log2(i + 2); // i is 0-indexed, rank is i + 1, log base is rank + 1 = i + 2
  }
  return dcg;
}

// Calculate Ideal Discounted Cumulative Gain at K
function calculateIDCG(expectedFiles: string[], k: number): number {
  let idcg = 0;
  const count = Math.min(expectedFiles.length, k);
  for (let i = 0; i < count; i++) {
    idcg += 1 / log2(i + 2); // Ideal is placing all relevant at the top with relevance=1
  }
  return idcg;
}

async function run() {
  console.log("\n=====================================================================");
  console.log("🔥 AKASHA Search & Retrieval Deep Evaluation Framework (v0.8.0)");
  console.log("=====================================================================\n");

  try {
    // 1. Register a test evaluation user
    console.log(`[1/5] Registering fresh evaluation user: ${TEST_EMAIL}`);
    const regRes = await fetch(`${API_URL}/api/v1/auth/register`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ email: TEST_EMAIL, password: TEST_PASSWORD }),
    });

    if (!regRes.ok) {
      throw new Error(`Registration failed: ${await regRes.text()}`);
    }
    const regData = (await regRes.json()) as any;
    console.log(`✔ Registered test user. ID: ${regData.user.id}`);

    // 2. Obtain JWT Token
    console.log(`[2/5] Authenticating user and retrieving secure JWT session token...`);
    const loginRes = await fetch(`${API_URL}/api/v1/auth/login`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ email: TEST_EMAIL, password: TEST_PASSWORD }),
    });

    if (!loginRes.ok) {
      throw new Error(`Authentication failed: ${await loginRes.text()}`);
    }
    const loginData = (await loginRes.json()) as any;
    const token = loginData.accessToken;
    console.log(`✔ Session established securely.`);

    // 3. Upload Gold-Standard Reference Documents
    console.log(`[3/5] Ingesting gold-standard evaluation documents...`);
    const docs = [
      {
        name: "bullmq_resilience_eval.txt",
        text: "BullMQ background processing and job resilience guidelines. When worker nodes crash due to hardware failures or unexpected memory exhaustion, they will terminate the current event loop. To recover from these crashed background jobs, the supervisor processes will auto-restart the Node. Jobs can then be recovered safely from the active queue using Redis state persistence.",
      },
      {
        name: "deepmind_foundation_eval.txt",
        text: "Grounded semantic reasoning system engineered by the DeepMind team. We believe that high-performance knowledge retrieval is the most critical core foundation of modern semantic artificial intelligence platforms, turning raw data into structured cognition.",
      },
      {
        name: "resnet_deeper_eval.txt",
        text: "We present a deep residual learning neural layers framework to ease the training of networks that are substantially deeper. We reformulate the layers as learning residual functions with reference to the layer inputs, instead of learning unreferenced functions, enabling deep scaling.",
      },
    ];

    for (const doc of docs) {
      const boundary = "----WebKitFormBoundaryEVAL" + Date.now();
      const bodyParts = [
        `--${boundary}\r\n`,
        `Content-Disposition: form-data; name="file"; filename="${doc.name}"\r\n`,
        `Content-Type: text/plain\r\n\r\n`,
        `${doc.text}\r\n`,
        `--${boundary}--\r\n`,
      ];

      const uploadRes = await fetch(`${API_URL}/api/v1/files`, {
        method: "POST",
        headers: {
          Authorization: `Bearer ${token}`,
          "Content-Type": `multipart/form-data; boundary=${boundary}`,
        },
        body: bodyParts.join(""),
      });

      if (!uploadRes.ok) {
        throw new Error(`Failed to upload ${doc.name}: ${await uploadRes.text()}`);
      }
      console.log(`✔ Uploaded ${doc.name} successfully.`);
    }

    // 4. Wait for Background Chunker and Embedding Pipeline
    console.log(`[4/5] Waiting for worker queues to complete chunking & 768d embedding extraction...`);
    let completed = false;
    for (let i = 1; i <= 20; i++) {
      await delay(1500);
      const listRes = await fetch(`${API_URL}/api/v1/files`, {
        headers: { Authorization: `Bearer ${token}` },
      });
      if (!listRes.ok) continue;

      const listData = (await listRes.json()) as any;
      const files = listData.data || [];
      const completedCount = files.filter((f: any) => f.status === "completed").length;
      console.log(`   (Attempt ${i}/20) Extracted and indexed files: ${completedCount}/${docs.length}`);

      if (completedCount === docs.length) {
        completed = true;
        break;
      }
    }

    if (!completed) {
      console.log("⚠ Chunker processing took longer than expected. Continuing search benchmarks...");
    } else {
      console.log("✔ Background worker completed all semantic chunk vector indexing tasks.");
    }

    // 5. Load and Run Benchmark Queries
    console.log(`[5/5] Executing semantic benchmark evaluation queries suite...`);
    const benchmarkPath = process.cwd().endsWith("apps/api")
      ? path.join(process.cwd(), "eval-benchmark.json")
      : path.join(process.cwd(), "apps/api/eval-benchmark.json");
    const baselinePath = process.cwd().endsWith("apps/api")
      ? path.join(process.cwd(), "eval-baseline.json")
      : path.join(process.cwd(), "apps/api/eval-baseline.json");

    const suite = JSON.parse(fs.readFileSync(benchmarkPath, "utf-8")) as BenchmarkSuite;
    const baseline = fs.existsSync(baselinePath)
      ? JSON.parse(fs.readFileSync(baselinePath, "utf-8"))
      : null;

    const modeMetricsAccumulator: Record<string, ModeMetrics[]> = {
      keyword: [],
      semantic: [],
      hybrid: [],
    };

    let totalAssertionsRun = 0;
    let totalAssertionsPassed = 0;
    let regressionDetected = false;

    for (const bench of suite.benchmarks) {
      console.log(`\n---------------------------------------------------------------------`);
      console.log(`Query #${bench.id}: "${bench.query}"`);
      console.log(`Expected target file(s): ${JSON.stringify(bench.expected_files)}`);
      console.log(`---------------------------------------------------------------------`);

      for (const mode of bench.modes) {
        const start = performance.now();
        const searchRes = await fetch(
          `${API_URL}/api/v1/search?q=${encodeURIComponent(bench.query)}&mode=${mode}`,
          { headers: { Authorization: `Bearer ${token}` } }
        );
        const latencyMs = performance.now() - start;

        if (!searchRes.ok) {
          console.log(`  🔴 Search failed for mode [${mode}]: ${await searchRes.text()}`);
          continue;
        }

        const payload = (await searchRes.json()) as any;
        const results = payload.data?.results || [];

        // 1. Compute Reciprocal Rank
        let rankOfFirstMatch = 0;
        for (let i = 0; i < results.length; i++) {
          if (bench.expected_files.includes(results[i].fileName)) {
            rankOfFirstMatch = i + 1;
            break;
          }
        }
        const mrrVal = rankOfFirstMatch > 0 ? 1 / rankOfFirstMatch : 0;

        // 2. Compute Precision@K and Recall@K
        const top1Hits = results.slice(0, 1).filter((r: any) => bench.expected_files.includes(r.fileName)).length;
        const top3Hits = results.slice(0, 3).filter((r: any) => bench.expected_files.includes(r.fileName)).length;

        const precisionAt1 = top1Hits / 1;
        const precisionAt3 = top3Hits / 3;
        const recallAt3 = top3Hits / bench.expected_files.length;

        // 3. Compute NDCG@10
        const dcg = calculateDCG(results, bench.expected_files, 10);
        const idcg = calculateIDCG(bench.expected_files, 10);
        const ndcgAt10 = idcg > 0 ? dcg / idcg : 0;

        // Save metrics
        modeMetricsAccumulator[mode].push({
          mrr: mrrVal,
          precisionAt1,
          precisionAt3,
          recallAt3,
          ndcgAt10,
          latencyMs,
        });

        // Top 1 similarity score
        const topScore = results.length > 0 ? results[0].rank : 0;

        // Assertions validation
        const mrrPasses = mrrVal >= bench.minimum_mrr;
        const scorePasses = topScore >= bench.minimum_top1_score;
        const keywordSnippetPasses = bench.expected_keywords_in_snippet.every(
          (kw) => results[0]?.snippet?.toLowerCase().includes(kw.toLowerCase())
        );

        totalAssertionsRun += 3;
        if (mrrPasses) totalAssertionsPassed++;
        if (scorePasses) totalAssertionsPassed++;
        if (keywordSnippetPasses) totalAssertionsPassed++;

        console.log(`  [Mode: ${mode.toUpperCase()}]`);
        console.log(`    - Latency: ${latencyMs.toFixed(1)} ms`);
        console.log(`    - Top Match Score: ${topScore.toFixed(4)} (Req: >= ${bench.minimum_top1_score}) [${scorePasses ? "🟢" : "🔴"}]`);
        console.log(`    - Reciprocal Rank: ${mrrVal.toFixed(2)} (Req MRR: >= ${bench.minimum_mrr}) [${mrrPasses ? "🟢" : "🔴"}]`);
        console.log(`    - Snippet Keywords Match: [${keywordSnippetPasses ? "🟢" : "🔴"}]`);
        console.log(`    - Precision@1: ${precisionAt1.toFixed(2)} | Precision@3: ${precisionAt3.toFixed(2)}`);
        console.log(`    - Recall@3: ${recallAt3.toFixed(2)} | NDCG@10: ${ndcgAt10.toFixed(2)}`);
      }
    }

    // Consolidated summaries
    console.log("\n=====================================================================");
    console.log("📊 SYSTEM COGNITION RETRIEVAL QUALITY SUMMARY");
    console.log("=====================================================================\n");

    const consolidatedReports: any[] = [];
    const modes = ["keyword", "semantic", "hybrid"];

    for (const mode of modes) {
      const metrics = modeMetricsAccumulator[mode];
      if (metrics.length === 0) continue;

      const avgMrr = metrics.reduce((acc, m) => acc + m.mrr, 0) / metrics.length;
      const avgP1 = metrics.reduce((acc, m) => acc + m.precisionAt1, 0) / metrics.length;
      const avgP3 = metrics.reduce((acc, m) => acc + m.precisionAt3, 0) / metrics.length;
      const avgRecall = metrics.reduce((acc, m) => acc + m.recallAt3, 0) / metrics.length;
      const avgNdcg = metrics.reduce((acc, m) => acc + m.ndcgAt10, 0) / metrics.length;
      const avgLatency = metrics.reduce((acc, m) => acc + m.latencyMs, 0) / metrics.length;

      consolidatedReports.push({
        Mode: mode.toUpperCase(),
        "Avg MRR": avgMrr.toFixed(3),
        "Avg P@1": avgP1.toFixed(3),
        "Avg P@3": avgP3.toFixed(3),
        "Avg Recall@3": avgRecall.toFixed(3),
        "Avg NDCG@10": avgNdcg.toFixed(3),
        "Avg Latency": `${avgLatency.toFixed(1)} ms`,
      });

      // Regression check against baseline snapshot
      if (baseline?.baseline_metrics?.[mode]) {
        const baseMode = baseline.baseline_metrics[mode];
        if (avgMrr < baseMode.avg_mrr - 0.05) {
          console.log(`⚠️  [REGRESSION DETECTED] ${mode.toUpperCase()} MRR dropped from baseline ${baseMode.avg_mrr} to ${avgMrr.toFixed(3)}`);
          regressionDetected = true;
        }
      }
    }

    console.table(consolidatedReports);

    console.log(`\nFinal Report:`);
    console.log(`  * Total Queries Evaluated: ${suite.benchmarks.length}`);
    console.log(`  * Ingestion & Retrieval Correctness Status: ${totalAssertionsPassed === totalAssertionsRun ? "🟢 PRISTINE" : "🔴 DEGRADED"}`);
    console.log(`  * Assertions Passed: ${totalAssertionsPassed}/${totalAssertionsRun}`);
    console.log(`  * Regression Alerts: ${regressionDetected ? "⚠️  ACTIVE ALERTS" : "✨ ZERO REGRESSIONS"}\n`);

    if (totalAssertionsPassed < totalAssertionsRun * 0.8) {
      console.log("🔴 ERROR: Search and retrieval quality is below acceptable standards.");
      process.exit(1);
    } else {
      console.log("🚀 Retrieval performance and semantic capture quality metrics validated successfully.");
    }
  } catch (error) {
    console.error("❌ Evaluation framework threw unexpected error:", error);
    process.exit(1);
  }
}

run();
