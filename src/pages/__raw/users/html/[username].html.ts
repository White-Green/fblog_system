import { getCollection } from 'astro:content';

export async function getStaticPaths() {
    const users = await getCollection('users');
    return users.map(user => {
        const username = user.id.split('/').pop()?.split('.')[0] || '';
        return { params: { username }, props: { user } };
    });
}

export const prerender = true;

export async function GET({ props }) {
    const { user } = props;
    const html = `<h1>${user.data.name}</h1><p>${user.data.bio}</p>`;
    return new Response(html, {
        headers: { 'Content-Type': 'text/html' }
    });
}
