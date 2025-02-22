import {glob} from 'astro/loaders';
import {defineCollection, z} from 'astro:content';

const articles = defineCollection({
    loader: glob({base: './contents/articles', pattern: '**/*.md'}),
    schema: z.object({
        title: z.string(),
        description: z.string(),
        pubDate: z.string().datetime({offset: true}),
        updatedDate: z.string().datetime({offset: true}).optional(),
        heroImage: z.string().optional(),
        author: z.string().optional(),
    }),
});

const users = defineCollection({
    loader: glob({base: './contents/users', pattern: '**/*.yml'}),
    schema: z.object({
        name: z.string(),
        bio: z.string(),
        avatar: z.string().optional(),
        website: z.string().url().optional(),
        twitter: z.string().optional(),
        github: z.string().optional(),
    }),
});

export const collections = {articles, users};
