import SwaggerUI from 'swagger-ui'
import 'swagger-ui/dist/swagger-ui.css';

const ui = SwaggerUI({
    configUrl: "/endpoints/ui-config.json",
    dom_id: '#ui'
})

ui.initOAuth({
    clientId: "drogue",
    scopes: "openid",
    additionalQueryStringParams: {nonce: "1"}
})