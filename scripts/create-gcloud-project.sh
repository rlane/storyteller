#!/bin/bash -eux
# This script creates a new Google Cloud project running Storyteller.
# Prerequisites:
#   - gcloud CLI installed and logged in
#   - A Google Cloud billing account
#   - Docker installed and logged in to gcloud
#   - An OpenAI API key in the OPENAI_API_KEY environment variable
which gcloud

PROJECT_ID=${1:-"storyteller-$(date +'%Y%m%d-%H%M%S')"}
REGION=us-west1
BILLING_ACCOUNT=$(gcloud beta billing accounts list --filter=open=true --limit=1 --format='value(name)')
test -n "$BILLING_ACCOUNT"
test -n "$OPENAI_API_KEY"

export CLOUDSDK_CORE_PROJECT=$PROJECT_ID
gcloud projects create $PROJECT_ID
gcloud beta billing projects link $PROJECT_ID --billing-account $BILLING_ACCOUNT
gcloud services enable artifactregistry.googleapis.com run.googleapis.com texttospeech.googleapis.com secretmanager.googleapis.com

gcloud iam service-accounts create storyteller-service
gcloud projects add-iam-policy-binding $PROJECT_ID --member="serviceAccount:storyteller-service@${PROJECT_ID}.iam.gserviceaccount.com" --role='roles/secretmanager.secretAccessor'

rm -rf .key
mkdir .key
chmod 0700 .key
trap "rm -rf .key" EXIT
gcloud iam service-accounts keys create .key/google-credentials.json --iam-account=storyteller-service@$PROJECT_ID.iam.gserviceaccount.com
gcloud secrets create google-credentials --data-file .key/google-credentials.json
rm -rf .key

echo $OPENAI_API_KEY | gcloud secrets create openai_api_key --data-file=-

while ! gcloud --project $PROJECT_ID artifacts repositories create services --repository-format=docker --location=$REGION; do
  sleep 10
done

: Created project $PROJECT_ID
