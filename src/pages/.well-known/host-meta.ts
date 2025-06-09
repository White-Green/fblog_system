export async function GET({params, props, request}) {
    const url = new URL(request.url);
    const baseUrl = `${url.protocol}//${url.host}`;
    return new Response(`"<?xml version="1.0"?><XRD xmlns="http://docs.oasis-open.org/ns/xri/xrd-1.0"><Link rel="lrdd" type="application/xrd+xml" template="${baseUrl}/.well-known/webfinger?resource={uri}"/></XRD>"`, {
        headers: {
            'Content-Type': 'application/xml'
        }
    });
}
