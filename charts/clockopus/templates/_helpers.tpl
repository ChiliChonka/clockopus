{{- define "clockopus.name" -}}
{{- .Release.Name | trunc 63 | trimSuffix "-" -}}
{{- end -}}

{{- define "clockopus.labels" -}}
app.kubernetes.io/name: clockopus
app.kubernetes.io/instance: {{ .Release.Name }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
helm.sh/chart: {{ printf "%s-%s" .Chart.Name .Chart.Version }}
{{- end -}}

{{- define "clockopus.selector" -}}
app.kubernetes.io/name: clockopus
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end -}}

{{- define "clockopus.cnpgName" -}}
{{- .Values.cnpg.name | default (printf "%s-db" (include "clockopus.name" .)) -}}
{{- end -}}

{{- /* Secret with the database URI: explicit, or the CNPG app secret. */ -}}
{{- define "clockopus.databaseSecret" -}}
{{- .Values.storage.postgres.existingSecret | default (printf "%s-app" (include "clockopus.cnpgName" .)) -}}
{{- end -}}
