{{/*
Create a Drogue IoT image name
*/}}
{{- define "drogue-cloud-core.image-repo" -}}
{{- with .Values.defaults.images.repository -}}
    {{ printf "%s/" . }}
{{- end }}
{{- end }}

{{/*
Expand the name of the chart.
*/}}
{{- define "drogue-cloud-core.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Create a default fully qualified app name.
We truncate at 63 chars because some Kubernetes name fields are limited to this (by the DNS naming spec).
If release name contains chart name it will be used as a full name.
*/}}
{{- define "drogue-cloud-core.fullname" -}}
{{- if .Values.fullnameOverride }}
{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- $name := default .Chart.Name .Values.nameOverride }}
{{- if contains $name .Release.Name }}
{{- .Release.Name | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- printf "%s-%s" .Release.Name $name | trunc 63 | trimSuffix "-" }}
{{- end }}
{{- end }}
{{- end }}

{{/*
Create chart name and version as used by the chart label.
*/}}
{{- define "drogue-cloud-core.chart" -}}
{{- printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Common labels
*/}}
{{- define "drogue-cloud-core.labels" -}}
helm.sh/chart: {{ include "drogue-cloud-core.chart" . }}
{{ include "drogue-cloud-core.selectorLabels" . }}
{{- if .Chart.AppVersion }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
{{- end }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
{{- end }}

{{/*
Selector labels
*/}}
{{- define "drogue-cloud-core.selectorLabels" -}}
app.kubernetes.io/name: {{ include "drogue-cloud-core.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}

{{/*
Passthrough service
*/}}
{{- define "drogue-cloud-core.service.passthrough.type" -}}

{{- if .Values.defaults.passthrough.type }}
{{ .Values.defaults.passthrough.type }}
{{- else }}

{{- if eq .Values.cluster "openshift" -}}
ClusterIP
{{- else if eq .Values.cluster "minikube" -}}
NodePort
{{- else if eq .Values.cluster "kind" -}}
NodePort
{{- else }}
LoadBalancer
{{- end }}
{{- end }}

{{- end }}
