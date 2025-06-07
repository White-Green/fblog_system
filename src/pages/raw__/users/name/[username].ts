import { getCollection } from 'astro:content';

export async function getStaticPaths() {
    const users = await getCollection('users');
    return users.map(user => {
        const username = user.id.split('/').pop()?.split('.')[0] || '';
        return { params: { username }, props: { user } };
    });
}

export const prerender = true;

export async function GET({ params }) {
    const { username } = params;
    return new Response(username, {
        headers: { 'Content-Type': 'text/plain' }
    });
}
