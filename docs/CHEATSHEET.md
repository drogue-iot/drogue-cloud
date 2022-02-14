# Cheatsheet

## Device registry

### Inspecting the PostgreSQL database

Run the following command to get access to the database:

    kubectl -n drogue-iot exec -it deployment/postgres -- bash -c 'psql -U $POSTGRES_USER -d $POSTGRES_DB'

## Profiling

### OpenShift

Create a service account:

```shell
oc create sa perf-account
```

Allow running as root:

```shell
oc adm policy add-scc-to-user privileged -z perf-account -n drogue-dev
```

You can remove this afterwards with:

```shell
oc adm policy remove-scc-from-user privileged -z perf-account -n drogue-dev
```

Or drop the service account using:

```shell
oc delete sa perf-account
```

### Adding the sidecar

In the deployment 

```yaml
spec:
  template:
    spec:
      shareProcessNamespace: true
      serviceAccountName: perf-account
      containers:
        - name: perf-sidecar
          image: ghcr.io/drogue-iot/perf-sidecar:0.1.0
          imagePullPolicy: IfNotPresent
          command: [ "sleep", "infinity" ]
          securityContext:
            privileged: true
          volumeMounts:
            - name: perf-output
              mountPath: /out
    volumes:
      - name: perf-output
        emptyDir: {}
```

### Running the tool

Check which PID the actual process is.

Run the performance tool:

```shell
cd /out
perf record --call-graph=lbr -p <pid>
```

Options:

```shell
-z # compress
--max-size 100M # limit output size
```

Some examples:

```shell
cd /out
perf record --call-graph=lbr -p 15 # works only on specific Intel CPUs 
perf record --call-graph=dwarf --max-size 100M -p 18
perf archive # archive all information
perf stat -g -p 15 -o /out/perf.data
```

### Getting the report

```shell
oc exec -i -c perf-sidecar deploy/mqtt-endpoint -- base64 /out/perf.data | base64 -d > perf.data
```

When using `archive`:

```shell
oc exec -i -c perf-sidecar deploy/mqtt-endpoint -- base64 /out/perf.data | base64 -d > perf.data
# and then the archive with symbols
oc exec -i -c perf-sidecar deploy/mqtt-endpoint -- base64 /out/perf.data.tar.bz2 | base64 -d > perf.data.tar.bz2 
tar xvf perf.data.tar.bz2 -C ~/.debug
```

### Flamegraph

Install: `cargo install flamegraph`

Then:

```shell
flamegraph --perfdata perf.data
```