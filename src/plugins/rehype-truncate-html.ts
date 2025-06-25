import {EXIT, visitParents} from 'unist-util-visit-parents';
import {visit} from 'unist-util-visit';
import type {Root, Text, Parent, Node, Element} from 'hast';

export function rehypeTruncateHtml(
  {limit = 1500, ellipsis = '…'}: {limit?: number; ellipsis?: string} = {}
) {
  return function transformer(tree: Root) {
    let count = 0;
    let done = false;

    // テキストノード全体の長さを先に計算しておく
    let total = 0;
    visit(tree, 'text', (node: Text) => {
      total += node.value.length;
    });

    // limit * 1.2 以内なら省略せずにそのまま表示する
    const threshold = limit * 1.2;
    if (total <= threshold) {
      return;
    }

    const omitted = total > limit ? total - limit : 0;

    /** 対象ノードより後ろを祖先まで遡って削除 */
    function cutAfter(node: Node, ancestors: Parent[]) {
      let child: Node = node;
      for (let i = ancestors.length - 1; i >= 0; i--) {
        const parent = ancestors[i];
        const idx = parent.children.indexOf(child);
        if (idx >= 0) parent.children.splice(idx + 1);
        child = parent;
      }
    }

    visitParents<Text>(tree, 'text', (node, ancestors) => {
      if (done) return EXIT;

      const remain = limit - count;
      const len = node.value.length;

      // --- 上限に達するケース ---
      if (len >= remain) {
        // 末尾へ省略記号を付けるため、あらかじめその分余裕を空ける
        const room = remain >= ellipsis.length ? remain - ellipsis.length : 0;
        node.value = node.value.slice(0, room) + ellipsis;

        // 省略された文字数を表示するメッセージを挿入
        if (omitted > 0) {
          const parent = ancestors[ancestors.length - 1];
          const idx = parent.children.indexOf(node);
          const messageNode: Element = {
            type: 'element',
            tagName: 'span',
            properties: {style: 'color:gray;font-style:italic'},
            children: [{type: 'text', value: `あと${omitted}文字省略されています`}],
          };
          parent.children.splice(idx + 1, 0, messageNode);
          cutAfter(messageNode, ancestors);
        } else {
          cutAfter(node, ancestors);
        }

        // 文字数カウントを上限に合わせる
        count = limit;
        done = true;
        return EXIT;
      }

      // --- まだ余裕がある ---
      count += len;
      return;
    });
  };
}
