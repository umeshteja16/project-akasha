#!/bin/bash
set -e

API_URL="http://localhost:3001"
PASSWORD="securepassword123"

EMAIL="user_special_$(date +%s)@akasha.com"

echo "=============================================="
echo "Testing Special Character Filename Ingestion..."
echo "=============================================="

echo "Registering test user: $EMAIL"
curl -s -X POST "$API_URL/api/v1/auth/register" \
  -H "Content-Type: application/json" \
  -d "{\"email\":\"$EMAIL\", \"password\":\"$PASSWORD\"}" > /dev/null

echo "Logging in..."
LOG_RES=$(curl -s -X POST "$API_URL/api/v1/auth/login" \
  -H "Content-Type: application/json" \
  -d "{\"email\":\"$EMAIL\", \"password\":\"$PASSWORD\"}")

TOKEN=$(echo "$LOG_RES" | grep -o '"accessToken":"[^"]*' | grep -o '[^"]*$')

if [ -z "$TOKEN" ]; then
  echo "❌ Login failed!"
  exit 1
fi

echo "Creating file with special character name..."
SPECIAL_FILENAME="Digital Product Design & Development Agency - Significa.html"
TEMP_FILE="/tmp/$SPECIAL_FILENAME"
echo "<html><body>Testing special characters like & in filename</body></html>" > "$TEMP_FILE"

echo "Uploading '$SPECIAL_FILENAME' to AKASHA API..."
UPLOAD_RES=$(curl -s -w "\nHTTP_CODE: %{http_code}\n" -X POST "$API_URL/api/v1/files" \
  -H "Authorization: Bearer $TOKEN" \
  -F "file=@$TEMP_FILE")

echo -e "\nResponse:"
echo "$UPLOAD_RES"

rm -f "$TEMP_FILE"

if echo "$UPLOAD_RES" | grep -q "HTTP_CODE: 201"; then
  echo "✅ SUCCESS: Special character filename uploaded successfully!"
  exit 0
else
  echo "❌ FAILURE: Ingestion failed."
  exit 1
fi
