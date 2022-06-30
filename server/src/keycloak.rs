use crate::config::ServerConfig;
use ::keycloak::types::CredentialRepresentation;
use std::collections::HashMap;

pub const SERVICE_CLIENT_SECRET: &str = "a73d4e96-461b-11ec-8d66-d45ddf138840";

pub fn configure_keycloak(config: &ServerConfig) {
    print!("Configuring keycloak... ");
    let server = &config.keycloak;
    let rt = tokio::runtime::Runtime::new().unwrap();
    let failed: usize = rt.block_on(async {
        let url = &server.url;
        let user = server.user.clone();
        let password = server.password.clone();
        let client = reqwest::Client::new();
        let admin_token = keycloak::KeycloakAdminToken::acquire(url, &user, &password, &client)
            .await
            .unwrap();
        let admin = keycloak::KeycloakAdmin::new(url, admin_token, client);

        let mut mapper_config = HashMap::new();
        mapper_config.insert("included.client.audience".into(), "drogue".into());
        mapper_config.insert("id.token.claim".into(), "false".into());
        mapper_config.insert("access.token.claim".into(), "true".into());
        let mappers = vec![keycloak::types::ProtocolMapperRepresentation {
            id: None,
            name: Some("add-audience".to_string()),
            protocol: Some("openid-connect".to_string()),
            protocol_mapper: Some("oidc-audience-mapper".to_string()),
            config: Some(mapper_config),
        }];

        let mut failed = 0;

        // configure the realm
        let r = keycloak::types::RealmRepresentation {
            realm: Some(server.realm.clone()),
            enabled: Some(true),
            ..Default::default()
        };

        if let Err(e) = admin.post(r).await {
            if let keycloak::KeycloakError::HttpFailure {
                status: 409,
                body: _,
                text: _,
            } = e
            {
                log::trace!("Realm 'drogue' already exists");
            } else {
                log::warn!("Error creating 'drogue' realm: {:?}", e);
                failed += 1;
            }
        }

        // Configure oauth account
        let mut c: keycloak::types::ClientRepresentation = Default::default();
        c.client_id.replace("drogue".to_string());
        c.enabled.replace(true);
        c.implicit_flow_enabled.replace(true);
        c.standard_flow_enabled.replace(true);
        c.direct_access_grants_enabled.replace(false);
        c.service_accounts_enabled.replace(false);
        c.full_scope_allowed.replace(true);
        c.root_url.replace("".to_string());
        c.redirect_uris.replace(vec!["*".to_string()]);
        c.web_origins.replace(vec!["*".to_string()]);
        c.client_authenticator_type
            .replace("client-secret".to_string());
        c.public_client.replace(true);
        c.secret.replace(SERVICE_CLIENT_SECRET.to_string());
        c.protocol_mappers.replace(mappers);

        if let Err(e) = admin.realm_clients_post(&server.realm, c).await {
            if let keycloak::KeycloakError::HttpFailure {
                status: 409,
                body: _,
                text: _,
            } = e
            {
                log::trace!("Client 'drogue' already exists");
            } else {
                log::warn!("Error creating 'drogue' client: {:?}", e);
                failed += 1;
            }
        }

        // Configure service account
        let mut c: keycloak::types::ClientRepresentation = Default::default();
        c.client_id.replace("services".to_string());
        c.implicit_flow_enabled.replace(false);
        c.standard_flow_enabled.replace(false);
        c.direct_access_grants_enabled.replace(false);
        c.service_accounts_enabled.replace(true);
        c.full_scope_allowed.replace(true);
        c.enabled.replace(true);
        c.client_authenticator_type
            .replace("client-secret".to_string());
        c.public_client.replace(false);
        c.secret.replace(SERVICE_CLIENT_SECRET.to_string());

        let mut mapper_config: HashMap<String, serde_json::value::Value> = HashMap::new();
        mapper_config.insert("included.client.audience".into(), "services".into());
        mapper_config.insert("id.token.claim".into(), "false".into());
        mapper_config.insert("access.token.claim".into(), "true".into());
        let mappers = vec![keycloak::types::ProtocolMapperRepresentation {
            id: None,
            name: Some("add-audience".to_string()),
            protocol: Some("openid-connect".to_string()),
            protocol_mapper: Some("oidc-audience-mapper".to_string()),
            config: Some(mapper_config),
        }];
        c.protocol_mappers.replace(mappers);

        if let Err(e) = admin.realm_clients_post(&server.realm, c).await {
            if let keycloak::KeycloakError::HttpFailure {
                status: 409,
                body: _,
                text: _,
            } = e
            {
                log::trace!("Client 'services' already exists");
            } else {
                log::warn!("Error creating 'services' client: {:?}", e);
                failed += 1;
            }
        }

        // Configure roles
        let mut admin_role = keycloak::types::RoleRepresentation::default();
        admin_role.name.replace("drogue-admin".to_string());
        if let Err(e) = admin
            .realm_roles_post(&server.realm, admin_role.clone())
            .await
        {
            if let keycloak::KeycloakError::HttpFailure {
                status: 409,
                body: _,
                text: _,
            } = e
            {
                log::trace!("Role 'drogue-admin' already exists");
            } else {
                log::warn!("Error creating 'drogue-admin' role: {:?}", e);
                failed += 1;
            }
        }

        let mut user_role = keycloak::types::RoleRepresentation::default();
        user_role.name.replace("drogue-user".to_string());
        if let Err(e) = admin
            .realm_roles_post(&server.realm, user_role.clone())
            .await
        {
            if let keycloak::KeycloakError::HttpFailure {
                status: 409,
                body: _,
                text: _,
            } = e
            {
                log::trace!("Role 'drogue-user' already exists");
            } else {
                log::warn!("Error creating 'drogue-user' role: {:?}", e);
                failed += 1;
            }
        }

        // Read back
        let user_role = admin
            .realm_roles_with_role_name_get(&server.realm, "drogue-user")
            .await;
        let admin_role = admin
            .realm_roles_with_role_name_get(&server.realm, "drogue-admin")
            .await;

        match (user_role, admin_role) {
            (Ok(user_role), Ok(admin_role)) => {
                // Add to default roles if not present
                if let Err(e) = admin
                    .realm_roles_with_role_name_composites_post(
                        &server.realm,
                        &format!("default-roles-{}", server.realm),
                        vec![admin_role, user_role],
                    )
                    .await
                {
                    log::warn!("Error associating roles with default: {:?}", e);
                    failed += 1;
                }
            }
            _ => {
                log::warn!("Error retrieving 'drogue-user' and 'drogue-admin' roles");
                failed += 1;
            }
        }

        // configure the admin user

        let u = keycloak::types::UserRepresentation {
            username: Some(config.drogue.admin_user.clone()),
            enabled: Some(true),
            credentials: Some(vec![CredentialRepresentation {
                type_: Some("password".into()),
                value: Some(config.drogue.admin_password.clone()),
                temporary: Some(false),
                ..Default::default()
            }]),
            ..Default::default()
        };

        if let Err(e) = admin.realm_users_post(&server.realm, u).await {
            if let keycloak::KeycloakError::HttpFailure {
                status: 409,
                body: _,
                text: _,
            } = e
            {
                log::trace!("User 'admin' already exists");
            } else {
                log::warn!("Error creating 'admin' user: {:?}", e);
                failed += 1;
            }
        }

        failed
    });

    if failed > 0 {
        println!("failed!");
    } else {
        println!("done!");
    }
}
