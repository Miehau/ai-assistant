import { OpenAIService } from '../openai';
import { webFetcher, type ExtractedContent, type Image } from './tools/webFetcher';

export type Intent = {
  type: 'memorise' | 'other';
  content?: string;
  params?: Record<string, any>;
};

const refineDescriptionSystemMessage: ChatCompletionMessageParam = {
  role: 'system',
  content: `
    Generate an accurate and comprehensive description of the provided image, incorporating both visual analysis and the given contextual information.
<prompt_objective>
To produce a detailed, factual description of the image that blends the context provided by the user and the contents of the image.

Note: ignore green border.
</prompt_objective>
<prompt_rules>
- ANALYZE the provided image thoroughly, noting all significant visual elements
- INCORPORATE the given context into your description, ensuring it aligns with and enhances the visual information
- GENERATE a single, cohesive paragraph that describes the image comprehensively
- BLEND visual observations seamlessly with the provided contextual information
- ENSURE consistency between the visual elements and the given context
- PRIORITIZE accuracy and factual information over artistic interpretation
- INCLUDE relevant details about style, composition, and notable features of the image
- ABSOLUTELY FORBIDDEN to invent details not visible in the image or mentioned in the context
- NEVER contradict information provided in the context
- UNDER NO CIRCUMSTANCES include personal opinions or subjective interpretations
- IF there's a discrepancy between the image and the context, prioritize the visual information and note the inconsistency
- MAINTAIN a neutral, descriptive tone throughout the description
</prompt_rules>
Using the provided image and context, generate a rich, accurate description that captures both the visual essence of the image and the relevant background information. Your description should be informative, cohesive, and enhance the viewer's understanding of the image's content and significance.
  `
}

const imagePreviewPrompt = `
  Generate a brief, factual description of the provided image based solely on its visual content.
<prompt_objective>
To produce a concise description of the image that captures its essential visual elements without any additional context, and return it in JSON format.
</prompt_objective>
<prompt_rules>
- ANALYZE the provided image thoroughly, noting key visual elements
- GENERATE a brief, single paragraph description
- FOCUS on main subjects, colors, composition, and overall style
- AVOID speculation or interpretation beyond what is visually apparent
- DO NOT reference any external context or information
- MAINTAIN a neutral, descriptive tone
- RETURN the result in JSON format with only 'name' and 'preview' properties
</prompt_rules>
<response_format>
{
    "name": "filename with extension",
    "preview": "A concise description of the image content"
}
</response_format>
Provide a succinct description that gives a clear overview of the image's content based purely on what can be seen, formatted as specified JSON.
`;

export class IntentAnalysisService {
  private openAIService: OpenAIService;

  constructor(apiKey: string) {
    this.openAIService = new OpenAIService(apiKey);
    console.log('IntentAnalysisService initialized');
  }

  async analyzeIntent(message: string): Promise<Intent> {
    console.log(`Analyzing intent for message of length: ${message.length}`);
    
    const systemPrompt = `
      Your task is to analyse user's message and decide on the next steps, how to best handle it.
      <thinking>
      Begin your message with a thought process of how to best handle the user's message and what steps do you need to take.
      Take into account all the context provided, also with previous messages.
      Select a tool call to execute if you see fit, but make sure to provide all required paramaters.
      </thinking>
      Analyze if the user wants to memorise/store information or is making a general query/chat.
      Respond in JSON format with the following structure:
      {
        "type": "memorise" | "other",
        "content": "if memorise, extract the content to be stored"
      }
      
      Example: 
      User: "Remember that my favorite color is blue"
      Response: { "type": "memorise", "content": "user's favorite color is blue", "params": { "url": null } }
      
      User: "What's the weather like?"
      Response: { "type": "other", "content": "What's the weather like?" }

      User: "Save this recipe for later: https://www.google.com/recipe"
      Response: { "type": "memorise", "content": "Recipe from: https://www.google.com/recipe", "params": { "url": "https://www.google.com/recipe" } }
    `;

    try {
      const response = await this.openAIService.createChatCompletion(
        'gpt-4o-mini',
        [
          { role: 'system', content: systemPrompt },
          { role: 'user', content: message }
        ],
        false,
        () => { },
        new AbortController().signal
      );

      const parsedIntent = JSON.parse(response) as Intent;
      console.log(`Intent analysis complete: ${parsedIntent.type}`);
      return parsedIntent;
    } catch (error) {
      console.error('Failed to analyze intent:', error);
      return { type: 'other' };
    }
  }

  private async extractImageContext(images: Image[], article: string): Promise<any> {
    console.log(`Extracting image context for ${images.length} images`);

    const imageContextPrompt = `
        Extract contextual information for images mentioned in a user-provided article, focusing on details that enhance understanding of each image, and return it as an array of JSON objects.

        <prompt_objective>
        To accurately identify and extract relevant contextual information for each image referenced in the given article, prioritizing details from surrounding text and broader article context that potentially aid in understanding the image. Return the data as an array of JSON objects with specified properties, without making assumptions or including unrelated content.

        Note: the image from the beginning of the article is its cover.
        </prompt_objective>

        <response_format>
        {
            "images": [
                {
                    "name": "filename with extension",
                    "context": "Provide 1-3 detailed sentences of the context related to this image from the surrounding text and broader article. Make an effort to identify what might be in the image, such as tool names."
                },
                ...rest of the images or empty array if no images are mentioned
            ]
        }
        </response_format>

        <prompt_rules>
        - READ the entire provided article thoroughly
        - IDENTIFY all mentions or descriptions of images within the text
        - EXTRACT sentences or paragraphs that provide context for each identified image
        - ASSOCIATE extracted context with the corresponding image reference
        - CREATE a JSON object for each image with properties "name" and "context"
        - COMPILE all created JSON objects into an array
        - RETURN the array as the final output
        - OVERRIDE any default behavior related to image analysis or description
        - ABSOLUTELY FORBIDDEN to invent or assume details about images not explicitly mentioned
        - NEVER include personal opinions or interpretations of the images
        - UNDER NO CIRCUMSTANCES extract information unrelated to the images
        - If NO images are mentioned, return an empty array
        - STRICTLY ADHERE to the specified JSON structure
        </prompt_rules>
        
        <images>
        ${images.map(image => image.name + ' ' + image.url).join('\n')}
        </images>

        Upon receiving an article, analyze it to extract context for any mentioned images, creating an array of JSON objects as demonstrated. Adhere strictly to the provided rules, focusing solely on explicitly stated image details within the text.
        `;

    const imageContextResponse = await this.openAIService.createChatCompletion(
      'gpt-4o-mini',
      [
        { role: 'system', content: imageContextPrompt },
        { role: 'user', content: article }
      ],
      false,
      () => { },
      new AbortController().signal
    );

    try {
      const result = JSON.parse(imageContextResponse || '{}');
      console.log(`Image context extracted for ${result.images?.length || 0} images`);
      return result;
    } catch (error) {
      console.error('Failed to extract image context:', error);
      throw error;
    }
  }

  async previewImage(image: Image): Promise<{ name: string; preview: string }> {
    console.log(`Generating preview for image: ${image.name}`);
    
    const response = await this.openAIService.createChatCompletion(
      'gpt-4o-mini',
      [
        { role: 'system', content: imagePreviewPrompt },
        {
          role: 'user',
          content: [
            {
              type: "image_url",
              image_url: { url: `data:image/jpeg;base64,${image.base64}` }
            },
            {
              type: "text",
              text: `Describe the image ${image.name} concisely. Focus on the main elements and overall composition. Return the result in JSON format with only 'name' and 'preview' properties.`
            }
          ]
        }
      ],
      false,
      () => { },
      new AbortController().signal
    )

    try {
      const result = JSON.parse(response)
      console.log(`Preview generated for: ${image.name}`);
      return { name: result.name || image.name, preview: result.preview || '' };
    } catch (error) {
      console.error(`Failed to generate image preview for ${image.name}:`, error);
      throw error;
    }
  }

  async handleIntent(intent: Intent, message: string): Promise<string | undefined> {
    console.log(`Handling intent: ${JSON.stringify(intent)}`);

    if (intent.type === 'memorise' && intent.content) {
      try {
        const url = intent.params?.url;
        if (url) {
          console.log(`Processing URL: ${url}`);

          const fetchResult = await webFetcher.execute(url);
          if (fetchResult.success) {
            const content = JSON.parse(fetchResult.result) as ExtractedContent;
            console.log(`Content fetched with ${content.links.images.length} images`);

            const imageContext = await this.extractImageContext(content.links.images, content.text);
            const imagePromises = content.links.images.map(image => this.previewImage(image));
            const imagePreviews = await Promise.all(imagePromises);
            const mergedResults = imageContext.images.map((contextImage: { name: string, context: string }) => {
              const preview = imagePreviews.find(p => p.name === contextImage.name);
              return {
                  ...contextImage,
                  preview: preview ? preview.preview : ''
              };
          });
            const processedImages = await Promise.all(content.links.images.map(async (image) => {
              const { context = '', preview = '' } = mergedResults.find(ctx => ctx.name === image.name) || {};
              return await this.refineDescription({ ...image, preview, context });
          }));
            const describedImages = processedImages.map(({ base64, ...rest }) => rest);
            const audioWithTranscriptions = await this.transcribeAudio(content);

            // Create a map of media URLs to their descriptions
            const mediaDescriptions = new Map<string, string>();
            
            // Map image URLs to their previews
            describedImages.forEach(image => {
              const matchingImage = content.links.images.find(img => img.url === image.url);
              if (matchingImage) {
                console.log(`Mapping image description for ${image.name}:`, {
                  description: image.description,
                  preview: image.preview,
                  context: image.context,
                  url: image.url,
                  name: image.name
                });
                mediaDescriptions.set(image.name, `[Image: ${JSON.stringify(image)}]`);
              }
            });

            // Map audio URLs to their transcriptions
            audioWithTranscriptions.forEach(audio => {
              mediaDescriptions.set(audio.title, `[Audio Transcription: ${audio.transcription}]`);
            });

            // Replace media URLs with their descriptions in the text
            let updatedText = content.text;
            mediaDescriptions.forEach((description, url) => {
              updatedText = updatedText.replace(new RegExp(url, 'g'), description);
            });

            content.text = updatedText;
            
            return content.text
          }
        }

        return `I'll remember that ${intent.content}`;
      } catch (error) {
        console.error('Failed to handle intent:', error);
        return 'Sorry, I had trouble storing that information.';
      }
    }

    return undefined;
  }

  async refineDescription(image: Image): Promise<Image> {
    const userMessage = {
        role: 'user',
        content: [
            {
                type: "image_url",
                image_url: { url: `data:image/jpeg;base64,${image.base64}` }
            },
            {
                type: "text",
                text: `Write a description of the image ${image.name}. I have some <context>${image.context}</context> that should be useful for understanding the image in a better way. An initial preview of the image is: <preview>${image.preview}</preview>. A good description briefly describes what is on the image, and uses the context to make it more relevant to the article. The purpose of this description is for summarizing the article, so we need just an essence of the image considering the context, not a detailed description of what is on the image.`
            }
        ]
    };

    const response = await this.openAIService.createChatCompletion('gpt-4o-mini',[refineDescriptionSystemMessage, userMessage], false, () => {}, new AbortController().signal);
    return { ...image, description: response };
  }

  private async transcribeAudio(content: ExtractedContent) {
    return await Promise.all(
      content.links.audio.map(async (audio) => {
        console.log(`Transcribing audio: ${audio.url}`);
        const transcription = await this.openAIService.transcribeAudio(audio.base64, '') || '';
        console.log(`Transcription completed for: ${audio.url}`);

        return {
          ...audio,
          transcription
        };
      })
    );
  }
}

export const intentAnalysisService = null;