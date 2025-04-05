declare module 'prismjs' {
  const Prism: {
    highlight: (code: string, grammar: any, language: string) => string;
    languages: Record<string, any>;
  };
  export default Prism;
}
