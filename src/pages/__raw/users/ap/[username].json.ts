import {getCollection} from 'astro:content';
import fs from 'node:fs';

function getPublicKeyPem(): string {
    if (!process.env.PUBLIC_KEY_FILE) {
        throw new Error('PUBLIC_KEY_FILE is not set');
    }
    return fs.readFileSync(process.env.PUBLIC_KEY_FILE, 'utf8');
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
