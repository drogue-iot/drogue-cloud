# Cheatsheet

## Device registry

### Inspecting the PostgreSQL database

Run the following command to get access to the database:

    kubectl -n drogue-iot exec -it deployment/postgres -- bash -c 'psql -U $POSTGRES_USER -d $POSTGRES_DB'
