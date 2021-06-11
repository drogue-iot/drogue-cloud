
{{/*
Ingress Host name
*/}}
{{- define "drogue-cloud-examples.ingress.hostname" -}}
{{- .Name }}.{{- .Context.Values.domain }}
{{- end }}

{{/*
SSO Host name
*/}}
{{- define "drogue-cloud-examples.sso.hostname" -}}
{{ include "drogue-cloud-examples.ingress.hostname" (dict "Context" . "Name" "keycloak" ) }}
{{- end }}

{{/*
SSO URL
*/}}
{{- define "drogue-cloud-examples.sso.url" -}}
{{- if eq .Values.cluster "openshift" }}https://{{- else }}http://{{- end }}
{{- include "drogue-cloud-examples.sso.hostname" . }}
{{- end }}