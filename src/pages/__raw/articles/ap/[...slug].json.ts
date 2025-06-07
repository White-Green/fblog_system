import {getCollection, getEntry} from 'astro:content';
import {unified} from 'unified';
import remarkParse from 'remark-parse';
import remarkBreaks from 'remark-breaks';
import remarkRehype from 'remark-rehype';
import rehypeStringify from 'rehype-stringify';
import {remarkCollectImages} from '../../../plugins/remark-collect-images';
import {rehypeTruncateHtml} from '../../../plugins/rehype-truncate-html';
import remarkMath from "remark-math";

export async function getStaticPaths() {
    const articles = await getCollection('articles');

    return articles.map(article => {
        // Use the path without the extension as the slug
        const slug = article.id.replace(/\.md$/, '');

        return {
            params: {slug},
            props: {article}
        };
    });
}

export const prerender = true;

export async function GET({params, props, request}) {
    const {slug} = params;
    const {article} = props;
    const url = new URL(request.url);
    const baseUrl = `${url.protocol}//${url.host}`;

    // Get the author information
    const authorId = article.data.author;
    const author = await getEntry('users', authorId);
    if (!author) {
        throw new Error(`Author not found: ${authorId}`);
    }

    // Convert markdown content to HTML and collect images
    const processor = unified()
        .use(remarkParse)
        .use(remarkBreaks)
        .use(remarkRehype)
        .use(remarkMath)
        .use(remarkCollectImages)
        .use(rehypeTruncateHtml)
        .use(rehypeStringify);

    const vFile = await processor.process(article.body);
    const contentHtml = `<a href="${baseUrl}/articles/${slug}"><strong>【${article.data.title}】</strong></a>${String(vFile)}`;

    // Extract image references from the collected data
    const attachments: any[] = [];
    if (vFile.data.images && Array.isArray(vFile.data.images)) {
        for (const image of vFile.data.images) {
            attachments.push({
                type: "Image",
                url: image.src.startsWith('http') ? image.src : `${baseUrl}${image.src}`,
                // Include additional metadata if available
                ...(image.alt && {name: image.alt}),
            });
        }
    }

    // Create the ActivityPub JSON for the article
    const articleJson = {
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": `${baseUrl}/articles/${slug}`,
        "type": "Note",
        "attributedTo": `${baseUrl}/users/${authorId}`,
        "content": contentHtml,
        "published": article.data.pubDate,
        "to": [
            "https://www.w3.org/ns/activitystreams#Public"
        ],
        "cc": [
            `${baseUrl}/users/${authorId}/followers`
        ],
        "attachment": attachments
    };

    return new Response(JSON.stringify(articleJson), {
        headers: {
            'Content-Type': 'application/json'
        }
    });
}
