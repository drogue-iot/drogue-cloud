function mapToDittoProtocolMsg(
    headers,
    textPayload,
    bytePayload,
    contentType
) {

    let application = headers["ce_application"];
    let device = headers["ce_device"];

    let datacontenttype = headers["content-type"];
    let dataschema = headers["ce_dataschema"];
    let type = headers["ce_type"];

    if (datacontenttype !== "application/json") {
        return null;
    }

    if (dataschema !== "urn:eclipse:ditto" ) {
        return null;
    }

    if (type !== "io.drogue.event.v1") {
        return null;
    }

    // let payload = JSON.parse(textPayload);

    let attributesObj = {
        drogue: {
            instance: headers["ce_instance"],
            application: headers["ce_application"],
            device: headers["ce_device"],
        }
    };

    let featuresObj = {
    };

    let dittoHeaders = {
        "response-required": false,
        "content-type": "application/merge-patch+json",
        "If-Match": "*"
    };

    return Ditto.buildDittoProtocolMsg(
        application,
        device,
        "things",
        "twin",
        "commands",
        "merge",
        "/",
        dittoHeaders,
        {
            attributes: attributesObj,
            features: featuresObj
        }
    );
}
