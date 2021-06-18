{{/*
Ingress Host name:

This takes an array of two values:
- The name of the service
- The .Values variable
*/}}
{{- define "drogue-cloud-common.ingress.hostname" -}}
{{- index . 1 }}{{- (index . 0).Values.global.domain }}
{{- end }}

{{/*
Ingress HTTP protocol
*/}}
{{- define "drogue-cloud-common.ingress.proto" -}}
{{- if eq .Values.global.cluster "openshift" }}https://{{- else }}http://{{- end }}
{{- end }}

{{/*
SSO Host name
*/}}
{{- define "drogue-cloud-common.sso.hostname" -}}
{{ include "drogue-cloud-common.ingress.hostname" (list . "keycloak" ) }}
{{- end }}

{{/*
SSO URL
*/}}
{{- define "drogue-cloud-common.sso.url" -}}
{{- include "drogue-cloud-common.ingress.proto" . }}
{{- include "drogue-cloud-common.sso.hostname" . }}
{{- end }}

{{/*
API Host name
*/}}
{{- define "drogue-cloud-common.api.hostname" -}}
{{ include "drogue-cloud-common.ingress.hostname" (list . "api" ) }}
{{- end }}

{{/*
API URL
*/}}
{{- define "drogue-cloud-common.api.url" -}}
{{- include "drogue-cloud-common.ingress.proto" . }}
{{- include "drogue-cloud-common.api.hostname" . }}
{{- end }}
