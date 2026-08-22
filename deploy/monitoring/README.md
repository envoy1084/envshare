# Monitoring integration

Scrape each node's loopback `/metrics` endpoint through a local Prometheus agent,
private network, or authenticated tunnel. Never publish the operations port to
the Internet. Give every target stable `region` and `failure_domain` labels; do
not use peer IDs, IP addresses, multiaddresses, or discovery namespaces as
labels.

Minimal Prometheus configuration shape:

```yaml
rule_files:
  - /etc/prometheus/rules/envshare-node.yaml

scrape_configs:
  - job_name: envshare-node
    static_configs:
      - targets: ["127.0.0.1:9100"]
        labels:
          region: replace-me
          failure_domain: replace-me
```

Install `prometheus-rules.yaml`, validate it with `promtool check rules`, and
reload Prometheus. Import `grafana-dashboard.json` as a Classic dashboard JSON
model and select the Prometheus data source.

The `envshare-node` rule group uses metrics emitted by the node itself. The
`envshare-node-platform-examples` group requires platform exporters:

- cAdvisor-compatible `container_memory_*` series for container memory;
- `process_open_fds` and `process_max_fds` from a process exporter; and
- Blackbox Exporter `probe_success` with module `envshare_node_ready` and a
  bounded `region` label for external regional checks.

Remove or adapt example rules whose prerequisite series are unavailable. Test
every alert by safely creating its condition in staging and confirm routing,
ownership, silence behavior, and runbook links before production use.
