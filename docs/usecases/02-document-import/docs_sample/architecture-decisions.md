---
author: Alice
date: 2025-10-15
---
# Architecture Decision Records

## ADR-001: Use PostgreSQL as primary database

We chose PostgreSQL over MySQL for its superior JSON support, full-text search
capabilities, and strong consistency guarantees. The decision was influenced by
our need for JSONB columns and advanced indexing.

## ADR-002: Kubernetes for container orchestration

Kubernetes was selected for its mature ecosystem. We use Helm charts for
deployment and ArgoCD for GitOps-based continuous delivery. All services
run as StatefulSets or Deployments with resource limits.

## ADR-003: Redis for caching layer

Redis provides sub-millisecond latency for session data and API response
caching. We evaluated Memcached but chose Redis for its data structure
support and pub/sub capabilities.
