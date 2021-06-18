{{/*
Create a Drogue IoT image name
*/}}
{{- define "drogue-cloud-core.image-repo" -}}
{{- with .Values.defaults.images.repository -}}
    {{ printf "%s/" . }}
{{- end }}
{{- end }}

{{/*
Image tag
*/}}
{{- define "drogue-cloud-core.image-tag" -}}
{{- .Values.defaults.images.tag | default .Chart.AppVersion }}
{{- end }}

{{/*
Expand the name of the chart.
*/}}
{{- define "drogue-cloud-core.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
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

{{- if eq .Values.global.cluster "openshift" -}}
ClusterIP
{{- else if eq .Values.global.cluster "minikube" -}}
NodePort
{{- else if eq .Values.global.cluster "kind" -}}
NodePort
{{- else -}}
LoadBalancer
{{- end }}
{{- end }}

{{- end }}
