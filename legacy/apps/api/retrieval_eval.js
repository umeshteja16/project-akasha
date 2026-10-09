import fs from "fs";
import path from "path";

const API_URL = "http://localhost:3001";
const TEST_EMAIL = `eval_runner_${Date.now()}@akasha.com`;
const TEST_PASSWORD = "super_secure_password_123";

async function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function runEvaluation() {
  console.log("\n========================================================");
  console.log("🚀 AKASHA High-Fidelity Retrieval Evaluation Framework");
  console.log("========================================================\n");

  try {
    // 1. Register test user
    console.log(`[1/5] Registering fresh test evaluation account: ${TEST_EMAIL}`);
    const regRes = await fetch(`${API_URL}/api/v1/auth/register`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ email: TEST_EMAIL, password: TEST_PASSWORD }),
    });

    if (!regRes.ok) {
      throw new Error(`User registration failed: ${await regRes.text()}`);
    }
    const regData = await regRes.json();
    console.log(`✔ Registered test user. ID: ${regData.user.id}`);

    // 2. Auth Login
    console.log(`[2/5] Authenticating and obtaining secure session token...`);
    const loginRes = await fetch(`${API_URL}/api/v1/auth/login`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ email: TEST_EMAIL, password: TEST_PASSWORD }),
    });

    if (!loginRes.ok) {
      throw new Error(`Authentication failed: ${await loginRes.text()}`);
    }
    const loginData = await loginRes.json();
    const token = loginData.accessToken;
    console.log(`✔ Authentication successful. Access token obtained.`);

    // 3. Upload Gold-Standard Evaluation Documents
    console.log(`[3/5] Ingesting gold-standard evaluation documents into storage...`);
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

    const uploadedIds = [];
    for (const doc of docs) {
      // Create multi-part mock upload body
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

      const uploadData = await uploadRes.json();
      uploadedIds.push(uploadData.data.id);
      console.log(`✔ Uploaded ${doc.name}. Ingestion Task ID: ${uploadData.data.id}`);
    }

    // 4. Wait for worker queue processing
    console.log(`[4/5] Waiting for background extraction worker pipelines to complete (10s)...`);
    let completedCount = 0;
    for (let attempt = 1; attempt <= 10; attempt++) {
      await delay(1500);
      const listRes = await fetch(`${API_URL}/api/v1/files`, {
        headers: { Authorization: `Bearer ${token}` },
      });
      const listData = await listRes.json();
      const files = listData.data || [];
      completedCount = files.filter((f) => f.status === "completed").length;
      console.log(`   (Attempt ${attempt}/10) Extraction progress: ${completedCount}/${docs.length} completed`);
      if (completedCount === docs.length) break;
    }

    if (completedCount < docs.length) {
      console.log("⚠ Ingestion processing is taking longer than expected. Proceeding with evaluation...");
    } else {
      console.log("✔ Background chunking, vector indexing, and auto-tagging successfully processed.");
    }

    // 5. Load and Run Benchmark Queries
    console.log(`[5/5] Launching benchmark evaluation queries suite...`);
    const benchmarkPath = path.join(process.cwd(), "apps/api/benchmark_queries.json");
    const { queries } = JSON.parse(fs.readFileSync(benchmarkPath, "utf-8"));

    const evalResults = [];
    let overallPassed = true;

    for (const suite of queries) {
      console.log(`\n--------------------------------------------------------`);
      console.log(`Evaluating Query: "${suite.query}" (Mode: ${suite.mode})`);
      console.log(`--------------------------------------------------------`);

      const start = performance.now();
      const searchRes = await fetch(
        `${API_URL}/api/v1/search?q=${encodeURIComponent(suite.query)}&mode=${suite.mode}`,
        { headers: { Authorization: `Bearer ${token}` } }
      );
      const latency = performance.now() - start;

      if (!searchRes.ok) {
        console.log(`✖ Search request failed: ${await searchRes.text()}`);
        overallPassed = false;
        continue;
      }

      const searchData = await searchRes.json();
      const results = searchData.data?.results || [];

      // Calculate precision parameters
      const hasHits = results.length > 0;
      let matchedSnippet = null;
      let score = 0;

      if (hasHits) {
        matchedSnippet = results[0].snippet;
        score = results[0].rank;
      }

      // Check expected keywords (Recall validation)
      const missedRecall = suite.expected_snippet_keywords.filter(
        (kw) => !matchedSnippet || !matchedSnippet.toLowerCase().includes(kw.toLowerCase())
      );

      // Check forbidden keywords (Precision / Noise isolation validation)
      const failedPrecision = suite.must_not_include_keywords.filter(
        (kw) => matchedSnippet && matchedSnippet.toLowerCase().includes(kw.toLowerCase())
      );

      const passesRecall = missedRecall.length === 0;
      const passesPrecision = failedPrecision.length === 0;
      const passesScore = score >= suite.minimum_score;

      const passed = passesRecall && passesPrecision && passesScore;
      if (!passed) overallPassed = false;

      console.log(`⚡ Telemetry Latency: ${latency.toFixed(2)} ms`);
      console.log(`🎯 Top Similarity Score: ${score.toFixed(4)} (Req: >= ${suite.minimum_score})`);
      console.log(`📂 Chunks Aggregated: ${results.length}`);
      
      if (hasHits) {
        console.log(`📝 Best Fragment: "${matchedSnippet.replace(/<mark>/g, "[").replace(/<\/mark>/g, "]")}"`);
      }

      console.log(`\nAssertions:`);
      console.log(`  [${passesScore ? "✔" : "✖"}] Confidence Score >= ${suite.minimum_score}`);
      console.log(`  [${passesRecall ? "✔" : "✖"}] Gold-standard Recall (Missed: ${JSON.stringify(missedRecall)})`);
      console.log(`  [${passesPrecision ? "✔" : "✖"}] Noise Exclusion Precision (Failed: ${JSON.stringify(failedPrecision)})`);
      
      evalResults.push({
        query: suite.query,
        latency,
        score,
        passed,
      });
    }

    // Print Consolidated Summary Table
    console.log("\n========================================================");
    console.log("📊 CONSOLIDATED EVALUATION REPORT SUMMARY");
    console.log("========================================================\n");
    
    console.table(
      evalResults.map((r) => ({
        "Benchmark Query": r.query.length > 30 ? r.query.slice(0, 27) + "..." : r.query,
        "Score": r.score.toFixed(4),
        "Latency": `${r.latency.toFixed(1)} ms`,
        "Result": r.passed ? "🟢 PASSED" : "🔴 FAILED",
      }))
    );

    const passedCount = evalResults.filter((r) => r.passed).length;
    const avgLatency = evalResults.reduce((acc, r) => acc + r.latency, 0) / evalResults.length;

    console.log(`\nMetrics:`);
    console.log(`  * Total Benchmark Queries: ${evalResults.length}`);
    console.log(`  * Passed Assertions: ${passedCount}/${evalResults.length}`);
    console.log(`  * Average Telemetry Latency: ${avgLatency.toFixed(2)} ms`);
    console.log(`  * Final Correctness Status: ${overallPassed ? "✨ PRISTINE RETRIEVAL SYSTEM" : "⚠ CORRUPTED OR DEGRADED RETRIEVAL QUALITY"}\n`);

    if (!overallPassed) {
      process.exit(1);
    }
  } catch (error) {
    console.error("✖ Evaluation runner threw unexpected crash error:", error);
    process.exit(1);
  }
}

runEvaluation();
