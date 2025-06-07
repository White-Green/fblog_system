import {visit} from 'unist-util-visit';
import type {Plugin} from 'unified';

interface ImageFile {
    src: string;
    alt?: string;
}

export function remarkCollectImages(): any {
    return (tree: any, file: any) => {
        const images: ImageFile[] = [];

        visit(tree, "element", (node: any) => {
            if (node.tagName != "img") return;
            // console.error(node);
            images.push({
                src: node.properties.src,
                alt: node.properties.alt
            });
        });

        // 必要に応じてメタデータとして記録
        file.data.images = images;
    };
}
