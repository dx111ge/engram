---
author: Alice
date: 2025-11-01
---
# Infrastructure Overview

Our production environment runs on Kubernetes. The primary database is PostgreSQL
running on dedicated nodes. Redis is used for session caching. Nginx serves as
the reverse proxy in front of all services.

PostgreSQL replication is managed by Patroni. Backups run nightly to S3.
The monitoring stack consists of Prometheus and Grafana.
