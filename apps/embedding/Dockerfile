FROM python:3.12-slim

WORKDIR /app

# Install system dependencies needed for sentence-transformers
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    && rm -rf /var/lib/apt/lists/*

# Copy requirements and install
COPY apps/embedding/requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt

# Pre-cache models during build stage for instant cold-start
ENV HF_HOME=/app/cached_models

# Primary embedding model: all-mpnet-base-v2 (768d, 109M params)
RUN python -c "from sentence_transformers import SentenceTransformer; SentenceTransformer('all-mpnet-base-v2', cache_folder='/app/cached_models')"

# Cross-encoder reranker: ms-marco-MiniLM-L-6-v2
RUN python -c "from sentence_transformers import CrossEncoder; CrossEncoder('cross-encoder/ms-marco-MiniLM-L-6-v2', cache_folder='/app/cached_models')"

# Copy application source code
COPY apps/embedding /app

# Harden process context: create non-root user and switch
RUN useradd -u 1000 -m appuser && chown -R appuser:appuser /app
USER appuser

EXPOSE 8000

CMD ["uvicorn", "main:app", "--host", "0.0.0.0", "--port", "8000"]
