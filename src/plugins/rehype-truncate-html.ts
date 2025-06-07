import {EXIT, visitParents} from 'unist-util-visit-parents';
type Root = any;
type Text = any;
type Parent = any;
type Node = any;

export function rehypeTruncateHtml(
  {limit = 1500, ellipsis = '…'}: {limit?: number; ellipsis?: string} = {}
) {
  return function transformer(tree: Root) {
    let count = 0;
    let done = false;

    /** 対象ノードより後ろを祖先まで遡って削除 */
    function cutAfter(node: any, ancestors: any[]) {
      let child: Node = node;
      for (let i = ancestors.length - 1; i >= 0; i--) {
        const parent = ancestors[i];
        const idx = parent.children.indexOf(child);
        if (idx >= 0) parent.children.splice(idx + 1);
        child = parent;
      }
    }

    visitParents(tree, 'text', (node: any, ancestors: any[]) => {
      if (done) return EXIT;

      const remain = limit - count;
      const len = node.value.length;

      // --- 上限に達するケース ---
      if (len >= remain) {
        // 末尾へ省略記号を付けるため、あらかじめその分余裕を空ける
        const room = remain >= ellipsis.length ? remain - ellipsis.length : 0;
        node.value = node.value.slice(0, room) + ellipsis;

        // 文字数カウントを上限に合わせる
        count = limit;
        cutAfter(node, ancestors);
        done = true;
        return EXIT;
      }

      // --- まだ余裕がある ---
      count += len;
      return;
    });
  };
}
