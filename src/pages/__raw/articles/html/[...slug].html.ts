import { getCollection } from 'astro:content';
import { unified } from 'unified';
import remarkParse from 'remark-parse';
import remarkBreaks from 'remark-breaks';
import remarkRehype from 'remark-rehype';
import remarkMath from 'remark-math';
import rehypeStringify from 'rehype-stringify';

export async function getStaticPaths() {
    const articles = await getCollection('articles');
    return articles.map(article => {
        const slug = article.id.replace(/\.md$/, '');
        return { params: { slug }, props: { article } };
    });
}

export const prerender = true;

export async function GET({ params, props }) {
    const { article } = props;
    const processor = unified()
        .use(remarkParse)
        .use(remarkBreaks)
        .use(remarkRehype)
        .use(remarkMath)
        .use(rehypeStringify);
    const html = String(await processor.process(article.body));
    return new Response(html, {
        headers: { 'Content-Type': 'text/html' }
    });
}
