#!/bin/bash -eux
# This script creates a new Google Cloud project running Storyteller.
# Prerequisites:
#   - gcloud CLI installed and logged in
#   - A Google Cloud billing account
#   - Docker installed and logged in to gcloud
which gcloud

PROJECT_ID=${1:-"storyteller-$(date +'%Y%m%d-%H%M%S')"}
REGION=us-west1
BILLING_ACCOUNT=$(gcloud beta billing accounts list --filter=open=true --limit=1 --format='value(name)')
test -n "$BILLING_ACCOUNT"

export CLOUDSDK_CORE_PROJECT=$PROJECT_ID
gcloud projects create $PROJECT_ID
gcloud beta billing projects link $PROJECT_ID --billing-account $BILLING_ACCOUNT
gcloud services enable artifactregistry.googleapis.com run.googleapis.com texttospeech.googleapis.com secretmanager.googleapis.com

gcloud iam service-accounts create storyteller-service
gcloud projects add-iam-policy-binding $PROJECT_ID --member="serviceAccount:storyteller-service@${PROJECT_ID}.iam.gserviceaccount.com" --role='roles/secretmanager.secretAccessor'

while ! gcloud --project $PROJECT_ID artifacts repositories create services --repository-format=docker --location=$REGION; do
  sleep 10
done
