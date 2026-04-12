#!/bin/bash

set -e

TIMESTAMP=$(date +"%Y%m%d_%H%M%S")
TAG="deploy_${TIMESTAMP}"

echo "Pushing any uncommitted changes and commits..."
git push origin main

echo "Creating deployment tag: ${TAG}"
git tag "${TAG}"
git push origin "${TAG}"

echo "Deployment tag ${TAG} pushed successfully!"
