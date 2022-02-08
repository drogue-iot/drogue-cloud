function mapToDittoProtocolMsg(
    headers,
    textPayload,
    bytePayload,
    contentType
) {

    let subject = headers["ce_subject"];

    let application = headers["ce_application"];
    let device = headers["ce_device"];

    let datacontenttype = headers["content-type"];
    let dataschema = headers["ce_dataschema"];
    let type = headers["ce_type"];
    let time = headers["ce_time"]

    if (datacontenttype !== "application/json" && !datacontenttype.endsWith("+json")) {
        return null;
    }

    if (dataschema !== "urn:eclipse:ditto"
            && datacontenttype !== "application/vnd.eclipse.ditto+json"
            && datacontenttype !== "application/merge-patch+json"
    ) {
        return null;
    }

    if (type !== "io.drogue.event.v1") {
        return null;
    }

    let payload = JSON.parse(textPayload);

    let dittoHeaders = {
        "response-required": false,
        "content-type": datacontenttype
    };

    if (typeof time === "string") {
        dittoHeaders["creation-time"] = Date.parse(time);
    }

    if (typeof payload["headers"] === "object") {
        dittoHeaders = Object.assign(dittoHeaders, payload["headers"]);
    }

    let path = payload["path"] || "/";
    let value = payload["value"] || {};

    return Ditto.buildDittoProtocolMsg(
        application,
        device,
        "things",
        "twin",
        "commands",
        subject,
        path,
        dittoHeaders,
        value
    );
}
