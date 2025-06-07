import { getCollection } from 'astro:content';

export async function getStaticPaths() {
    const articles = await getCollection('articles');
    return articles.map(article => {
        const slug = article.id.replace(/\.md$/, '');
        return { params: { slug }, props: { article } };
    });
}

export const prerender = true;

export async function GET({ props }) {
    const { article } = props;
    const authorId = article.data.author || '';
    return new Response(authorId, {
        headers: { 'Content-Type': 'text/plain' }
    });
}
