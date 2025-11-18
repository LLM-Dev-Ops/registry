#!/usr/bin/env node

import { Command } from 'commander';
import { LLMRegistryClient, Model, Asset } from '@llm-dev-ops/llm-registry-sdk';
import chalk from 'chalk';
import ora from 'ora';
import { table } from 'table';
import * as fs from 'fs';
import * as path from 'path';

const program = new Command();

// Configuration
interface Config {
  baseURL?: string;
  apiToken?: string;
}

let config: Config = {};

// Load config from file or environment
function loadConfig(): Config {
  const configPath = path.join(process.env.HOME || '', '.llm-registry.json');

  if (fs.existsSync(configPath)) {
    try {
      config = JSON.parse(fs.readFileSync(configPath, 'utf-8'));
    } catch (err) {
      console.error(chalk.yellow('Warning: Could not load config file'));
    }
  }

  return {
    baseURL: process.env.LLM_REGISTRY_URL || config.baseURL || 'http://localhost:8080',
    apiToken: process.env.LLM_REGISTRY_TOKEN || config.apiToken,
  };
}

// Get client instance
function getClient(): LLMRegistryClient {
  const cfg = loadConfig();
  if (!cfg.baseURL) {
    console.error(chalk.red('Error: No base URL configured. Set LLM_REGISTRY_URL environment variable or use config command.'));
    process.exit(1);
  }
  return new LLMRegistryClient({
    baseURL: cfg.baseURL,
    apiToken: cfg.apiToken,
  });
}

program
  .name('llm-registry')
  .description('CLI for the LLM Registry')
  .version('0.1.0');

// Config command
program
  .command('config')
  .description('Configure the CLI')
  .option('-u, --url <url>', 'Set the base URL')
  .option('-t, --token <token>', 'Set the API token')
  .action((options) => {
    const configPath = path.join(process.env.HOME || '', '.llm-registry.json');

    if (options.url) {
      config.baseURL = options.url;
    }
    if (options.token) {
      config.apiToken = options.token;
    }

    fs.writeFileSync(configPath, JSON.stringify(config, null, 2));
    console.log(chalk.green('Configuration saved!'));
    console.log(chalk.gray(`Base URL: ${config.baseURL}`));
    console.log(chalk.gray(`Token: ${config.apiToken ? '***' : 'Not set'}`));
  });

// Model commands
const models = program.command('models').description('Manage models');

models
  .command('list')
  .description('List all models')
  .option('-p, --provider <provider>', 'Filter by provider')
  .option('-t, --tags <tags>', 'Filter by tags (comma-separated)')
  .option('-l, --limit <limit>', 'Limit results', '10')
  .action(async (options) => {
    const spinner = ora('Fetching models...').start();
    try {
      const client = getClient();
      const modelList = await client.listModels({
        provider: options.provider,
        tags: options.tags?.split(','),
        limit: parseInt(options.limit),
      });

      spinner.stop();

      if (modelList.length === 0) {
        console.log(chalk.yellow('No models found'));
        return;
      }

      const data = [
        ['ID', 'Name', 'Version', 'Provider', 'Created'],
        ...modelList.map((m: Model) => [
          m.id.substring(0, 8),
          m.name,
          m.version,
          m.provider || 'N/A',
          new Date(m.created_at).toLocaleDateString(),
        ]),
      ];

      console.log(table(data));
    } catch (err: any) {
      spinner.fail('Failed to fetch models');
      console.error(chalk.red(err.message));
      process.exit(1);
    }
  });

models
  .command('get <id>')
  .description('Get model details')
  .action(async (id) => {
    const spinner = ora('Fetching model...').start();
    try {
      const client = getClient();
      const model = await client.getModel(id);
      spinner.stop();

      console.log(chalk.bold('\nModel Details:'));
      console.log(chalk.gray('ID:'), model.id);
      console.log(chalk.gray('Name:'), model.name);
      console.log(chalk.gray('Version:'), model.version);
      console.log(chalk.gray('Description:'), model.description || 'N/A');
      console.log(chalk.gray('Provider:'), model.provider || 'N/A');
      console.log(chalk.gray('Tags:'), model.tags?.join(', ') || 'None');
      console.log(chalk.gray('Created:'), new Date(model.created_at).toLocaleString());
      console.log(chalk.gray('Updated:'), new Date(model.updated_at).toLocaleString());

      if (model.metadata) {
        console.log(chalk.gray('Metadata:'), JSON.stringify(model.metadata, null, 2));
      }
    } catch (err: any) {
      spinner.fail('Failed to fetch model');
      console.error(chalk.red(err.message));
      process.exit(1);
    }
  });

models
  .command('create')
  .description('Create a new model')
  .requiredOption('-n, --name <name>', 'Model name')
  .requiredOption('-v, --version <version>', 'Model version')
  .option('-d, --description <description>', 'Model description')
  .option('-p, --provider <provider>', 'Model provider')
  .option('-t, --tags <tags>', 'Tags (comma-separated)')
  .action(async (options) => {
    const spinner = ora('Creating model...').start();
    try {
      const client = getClient();
      const model = await client.createModel({
        name: options.name,
        version: options.version,
        description: options.description,
        provider: options.provider,
        tags: options.tags?.split(','),
      });

      spinner.succeed('Model created successfully!');
      console.log(chalk.gray('ID:'), model.id);
      console.log(chalk.gray('Name:'), model.name);
      console.log(chalk.gray('Version:'), model.version);
    } catch (err: any) {
      spinner.fail('Failed to create model');
      console.error(chalk.red(err.message));
      process.exit(1);
    }
  });

models
  .command('delete <id>')
  .description('Delete a model')
  .action(async (id) => {
    const spinner = ora('Deleting model...').start();
    try {
      const client = getClient();
      await client.deleteModel(id);
      spinner.succeed('Model deleted successfully!');
    } catch (err: any) {
      spinner.fail('Failed to delete model');
      console.error(chalk.red(err.message));
      process.exit(1);
    }
  });

// Asset commands
const assets = program.command('assets').description('Manage assets');

assets
  .command('list <model-id>')
  .description('List assets for a model')
  .action(async (modelId) => {
    const spinner = ora('Fetching assets...').start();
    try {
      const client = getClient();
      const assetList = await client.listAssets(modelId);

      spinner.stop();

      if (assetList.length === 0) {
        console.log(chalk.yellow('No assets found'));
        return;
      }

      const data = [
        ['ID', 'Name', 'Version', 'Type', 'Size', 'Created'],
        ...assetList.map((a: Asset) => [
          a.id.substring(0, 8),
          a.name,
          a.version,
          a.content_type,
          `${(a.size / 1024 / 1024).toFixed(2)} MB`,
          new Date(a.created_at).toLocaleDateString(),
        ]),
      ];

      console.log(table(data));
    } catch (err: any) {
      spinner.fail('Failed to fetch assets');
      console.error(chalk.red(err.message));
      process.exit(1);
    }
  });

assets
  .command('upload <model-id> <file>')
  .description('Upload an asset')
  .requiredOption('-n, --name <name>', 'Asset name')
  .requiredOption('-v, --version <version>', 'Asset version')
  .option('-t, --content-type <type>', 'Content type', 'application/octet-stream')
  .action(async (modelId, file, options) => {
    const spinner = ora('Uploading asset...').start();
    try {
      const client = getClient();
      const fileBuffer = fs.readFileSync(file);

      const asset = await client.uploadAsset({
        model_id: modelId,
        name: options.name,
        version: options.version,
        content_type: options.contentType,
        file: fileBuffer,
      });

      spinner.succeed('Asset uploaded successfully!');
      console.log(chalk.gray('ID:'), asset.id);
      console.log(chalk.gray('Name:'), asset.name);
      console.log(chalk.gray('Size:'), `${(asset.size / 1024 / 1024).toFixed(2)} MB`);
    } catch (err: any) {
      spinner.fail('Failed to upload asset');
      console.error(chalk.red(err.message));
      process.exit(1);
    }
  });

assets
  .command('download <model-id> <asset-id> <output>')
  .description('Download an asset')
  .action(async (modelId, assetId, output) => {
    const spinner = ora('Downloading asset...').start();
    try {
      const client = getClient();
      const data = await client.downloadAsset(modelId, assetId);

      fs.writeFileSync(output, Buffer.from(data));
      spinner.succeed(`Asset downloaded to ${output}`);
    } catch (err: any) {
      spinner.fail('Failed to download asset');
      console.error(chalk.red(err.message));
      process.exit(1);
    }
  });

// Health command
program
  .command('health')
  .description('Check API health')
  .action(async () => {
    const spinner = ora('Checking health...').start();
    try {
      const client = getClient();
      const health = await client.health();
      spinner.succeed('API is healthy!');
      console.log(chalk.gray('Status:'), health.status);
      if (health.version) {
        console.log(chalk.gray('Version:'), health.version);
      }
    } catch (err: any) {
      spinner.fail('API is not healthy');
      console.error(chalk.red(err.message));
      process.exit(1);
    }
  });

program.parse();
