---
author: Charlie
date: 2026-02-10
---
# Deployment Guide

## Prerequisites

Ensure Terraform has provisioned the infrastructure. Verify that the
Kubernetes cluster is healthy with kubectl. Docker images must be pushed
to our container registry before deployment.

## Deployment Process

1. Merge PR to main branch
2. Jenkins pipeline builds Docker images
3. ArgoCD detects the new image tag and syncs
4. Kubernetes rolls out the new version with zero downtime
5. Prometheus alerts verify health after deployment

## Rollback

If Grafana dashboards show error rate above 1%, trigger rollback via
ArgoCD. The previous ReplicaSet is still available in Kubernetes.
