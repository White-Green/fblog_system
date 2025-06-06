import {getCollection} from 'astro:content';
import fs from 'node:fs';

const DEFAULT_PUBLIC_KEY =
    "-----BEGIN PUBLIC KEY-----\n" +
    "MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAyF6RgWJwN+xSgGhZmV3j\n" +
    "ayGyFpL6gt02RIkTuSQeHxaCz/cBepb5B1Xj5g5sifVLyq9lJh1S9VRfn1iOsCiS\n" +
    "G9JNSkvELhuWYXqbTJbr1n7P/NdofWKJc4QQessZ41rnojHHmjcMjW3Q4R3Xwe0D\n" +
    "RSSKIqCfcp+8wWzoFDhGN327scTK9XlMee8acaWvzKBg6gZxEEh4u03+Rzngty9L\n" +
    "dMx07nHx+af2qVvzLgnrqPwOmSqimFSoUmHErC/UjSTF87/ex5kcY/RWyyPNyyQX\n" +
    "l087CpLVON3NqShC4ftFrmR0TAAHyZTQZxF/Tn5WgRv2DwXaDMTdC+T7zR7MrqBI\n" +
    "mwIDAQAB\n" +
    "-----END PUBLIC KEY-----\n";

function getPublicKeyPem(): string {
    if (process.env.PUBLIC_KEY_FILE) {
        try {
            return fs.readFileSync(process.env.PUBLIC_KEY_FILE, 'utf8');
        } catch (e) {
            console.error(`Failed to read public key from ${process.env.PUBLIC_KEY_FILE}`, e);
        }
    }
    return DEFAULT_PUBLIC_KEY;
}

export async function getStaticPaths() {
    const users = await getCollection('users');

    return users.map(user => {
        // Extract the username from the file path (e.g., 'default.yml' -> 'default')
        const username = user.id.split('/').pop()?.split('.')[0] || '';

        return {
            params: {username},
            props: {user}
        };
    });
}

export async function GET({params, request}) {
    const {username} = params;
    const url = new URL(request.url);
    const baseUrl = `${url.protocol}//${url.host}`;

    // Create the ActivityPub JSON for the user
    const userJson = {
        "@context": [
            "https://www.w3.org/ns/activitystreams"
        ],
        "type": "Person",
        "id": `${baseUrl}/users/${username}`,
        "inbox": `${baseUrl}/users/${username}/inbox`,
        "outbox": `${baseUrl}/users/${username}/outbox`,
        "following": `${baseUrl}/users/${username}/following`,
        "followers": `${baseUrl}/users/${username}/followers`,
        "preferredUsername": username,
        "publicKey": {
            "id": `${baseUrl}/users/${username}#main-key`,
            "type": "Key",
            "owner": `${baseUrl}/users/${username}`,
            "publicKeyPem": getPublicKeyPem(),
        }
    };

    return new Response(JSON.stringify(userJson), {
        headers: {
            'Content-Type': 'application/json'
        }
    });
}
