import os
from fastapi import FastAPI, HTTPException
from pydantic import BaseModel
from sentence_transformers import SentenceTransformer, CrossEncoder
from typing import List

app = FastAPI(title="AKASHA Embedding & Reranking Service")

# Use same cache directory set up during Docker build
CACHE_DIR = os.environ.get("HF_HOME", "/app/cached_models")

# Primary embedding model: all-mpnet-base-v2 (768 dimensions, 109M params)
# Significantly better semantic understanding than all-MiniLM-L6-v2 on MTEB/BEIR benchmarks
EMBED_MODEL_NAME = "all-mpnet-base-v2"
EMBED_DIMENSIONS = 768

print(f"Loading SentenceTransformer {EMBED_MODEL_NAME} from cache directory: {CACHE_DIR}")
model = SentenceTransformer(EMBED_MODEL_NAME, cache_folder=CACHE_DIR)

print(f"Loading CrossEncoder ms-marco-MiniLM-L-6-v2 from cache directory: {CACHE_DIR}")
reranker = CrossEncoder('cross-encoder/ms-marco-MiniLM-L-6-v2', cache_folder=CACHE_DIR)
print("Models loaded successfully!")

class EmbeddingRequest(BaseModel):
    texts: List[str]

class EmbeddingResponse(BaseModel):
    embeddings: List[List[float]]

class RerankRequest(BaseModel):
    query: str
    documents: List[str]

class RerankResult(BaseModel):
    index: int
    score: float

class RerankResponse(BaseModel):
    results: List[RerankResult]

@app.post("/embed", response_model=EmbeddingResponse)
async def generate_embeddings(request: EmbeddingRequest):
    if not request.texts:
        return EmbeddingResponse(embeddings=[])
    try:
        BATCH_SIZE = 32
        all_embeddings = []
        for i in range(0, len(request.texts), BATCH_SIZE):
            batch = request.texts[i:i + BATCH_SIZE]
            embeddings = model.encode(batch, show_progress_bar=False)
            all_embeddings.extend([emb.tolist() for emb in embeddings])
        return EmbeddingResponse(embeddings=all_embeddings)
    except Exception as e:
        print(f"Embedding error: {str(e)}")
        raise HTTPException(status_code=500, detail=str(e))

@app.post("/rerank", response_model=RerankResponse)
async def rerank_documents(request: RerankRequest):
    if not request.documents:
        return RerankResponse(results=[])
    try:
        # Predict relevancy score for each query-document pair
        pairs = [[request.query, doc] for doc in request.documents]
        scores = reranker.predict(pairs)
        results = [
            RerankResult(index=i, score=float(score))
            for i, score in enumerate(scores)
        ]
        return RerankResponse(results=results)
    except Exception as e:
        print(f"Reranking error: {str(e)}")
        raise HTTPException(status_code=500, detail=str(e))

@app.get("/health")
async def health():
    return {
        "status": "ok",
        "model": EMBED_MODEL_NAME,
        "reranker": "ms-marco-MiniLM-L-6-v2",
        "dimensions": EMBED_DIMENSIONS
    }

@app.get("/info")
async def info():
    return {
        "model": EMBED_MODEL_NAME,
        "dimensions": EMBED_DIMENSIONS,
        "max_seq_length": model.max_seq_length,
        "reranker": "cross-encoder/ms-marco-MiniLM-L-6-v2",
    }
