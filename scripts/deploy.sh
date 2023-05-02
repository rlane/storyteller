#!/bin/bash -eux
REGION=us-west1
PROJECT_ID=storyteller-20230501-184513
IMAGE=$REGION-docker.pkg.dev/$PROJECT_ID/services/storyteller
scripts/build-docker.sh
docker tag storyteller $IMAGE
docker push $IMAGE

gcloud --project $PROJECT_ID run deploy storyteller \
  --image $IMAGE \
  --allow-unauthenticated \
  --region $REGION \
  --execution-environment=gen2 \
  --cpu=2 \
  --memory=2G \
  --timeout=300s \
  --concurrency=10 \
  --max-instances=10 \
  --service-account=storyteller-service@$PROJECT_ID.iam.gserviceaccount.com \
  --set-env-vars=GOOGLE_APPLICATION_CREDENTIALS=/secrets/google-credentials.json \
  --update-secrets=/secrets/google-credentials.json=google-credentials:latest,OPENAI_API_KEY=openai_api_key:latest
