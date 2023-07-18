import { Visitor } from '@swc/core/Visitor'

class TransformVisitor extends Visitor {
  visitCallExpression(n) {
    if (n?.callee?.object?.value === "console") {
      if (n.arguments.length >= 1) {
        n.arguments[0].expression.value = "from_plugin";
        n.arguments[0].expression.raw = `"from_plugin"`;
      }
    }

    return n;
  }
}