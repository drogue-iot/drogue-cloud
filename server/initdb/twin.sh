: "${DOPPELGAENGER_DATABASE:=doppelgaenger}"

# Create an additional database for Doppelgaenger

echo "CREATE DATABASE \"${DOPPELGAENGER_DATABASE}\";" | postgresql_execute "" "postgres" ""
echo "GRANT ALL PRIVILEGES ON DATABASE \"${DOPPELGAENGER_DATABASE}\" TO \"${POSTGRESQL_USERNAME}\"\;" | postgresql_execute "" "postgres" "$POSTGRESQL_PASSWORD"
echo "ALTER DATABASE \"${DOPPELGAENGER_DATABASE}\" OWNER TO \"${POSTGRESQL_USERNAME}\"\;" | postgresql_execute "" "postgres" "${POSTGRESQL_PASSWORD}"
echo "ALTER SCHEMA public OWNER TO \"${POSTGRESQL_USERNAME}\"\;" | postgresql_execute "${DOPPELGAENGER_DATABASE}" "postgres" "${POSTGRESQL_PASSWORD}"
