#!/bin/bash
set -e

API_URL="http://localhost:3001"
EMAIL="test_security_$(date +%s)@akasha.com"
PASSWORD="securepassword123"

echo "=== AKASHA Magic Bytes Security Hardening Integration Test ==="

# 1. Register and Login
echo "Registering test user: $EMAIL"
curl -s -X POST "$API_URL/api/v1/auth/register" \
  -H "Content-Type: application/json" \
  -d "{\"email\":\"$EMAIL\", \"password\":\"$PASSWORD\"}" > /dev/null

echo "Logging in..."
LOGIN_RES=$(curl -s -X POST "$API_URL/api/v1/auth/login" \
  -H "Content-Type: application/json" \
  -d "{\"email\":\"$EMAIL\", \"password\":\"$PASSWORD\"}")

ACCESS_TOKEN=$(echo "$LOGIN_RES" | grep -o '"accessToken":"[^"]*' | grep -o '[^"]*$')

if [ -z "$ACCESS_TOKEN" ]; then
  echo "Error: Authentication failed."
  exit 1
fi
echo "Authenticated successfully."

# 2. Test Case A: Upload a fake PDF (masquerading plaintext script)
# Mimetype sent by curl is application/pdf, but file content doesn't start with %PDF
echo "creating fake PDF..."
echo "echo 'Malicious binary masquerader!'" > /tmp/fake_malicious.pdf

echo "Uploading fake PDF (Disguised plaintext script)..."
HTTP_CODE_A=$(curl -s -o /tmp/fake_pdf_res.txt -w "%{http_code}" -X POST "$API_URL/api/v1/files" \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -F "file=@/tmp/fake_malicious.pdf;type=application/pdf")

echo "Fake PDF Upload HTTP Response Code: $HTTP_CODE_A"
echo "Fake PDF Response Content: $(cat /tmp/fake_pdf_res.txt)"

if [ "$HTTP_CODE_A" -eq 415 ]; then
  echo "✅ PASS: Fake PDF upload blocked successfully with HTTP 415!"
else
  echo "❌ FAIL: Expected HTTP 415, got $HTTP_CODE_A"
  exit 1
fi

# 3. Test Case B: Upload a real PDF (starts with %PDF)
echo "creating valid PDF..."
printf "%%PDF-1.4\n1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n" > /tmp/valid_real.pdf

echo "Uploading valid PDF..."
HTTP_CODE_B=$(curl -s -o /tmp/valid_pdf_res.txt -w "%{http_code}" -X POST "$API_URL/api/v1/files" \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -F "file=@/tmp/valid_real.pdf;type=application/pdf")

echo "Valid PDF Upload HTTP Response Code: $HTTP_CODE_B"
if [ "$HTTP_CODE_B" -eq 201 ]; then
  echo "✅ PASS: Valid PDF upload allowed successfully!"
else
  echo "❌ FAIL: Expected HTTP 201, got $HTTP_CODE_B"
  exit 1
fi

# Clean up
rm -f /tmp/fake_malicious.pdf /tmp/valid_real.pdf /tmp/fake_pdf_res.txt /tmp/valid_pdf_res.txt
echo "=== Magic Bytes Hardening Test Passed Successfully! ==="
