using System;
using Microsoft.AspNetCore.Hosting;
using Microsoft.Extensions.Hosting;
using RibbleChatServer.Utils;
using System.Threading.Tasks;

namespace RibbleChatServer
{
    public class Program
    {
        public static Task Main(string[] args) =>
            CreateHostBuilder(args).Build().RunAsync();

        public static IHostBuilder CreateHostBuilder(string[] args) =>
            Host.CreateDefaultBuilder(args)
                .ConfigureWebHostDefaults(webBuilder =>
                {
                    webBuilder.UseStartup<Startup>();
                    var port = Environment.GetEnvironmentVariable("PORT")?.Map(Int32.Parse) ?? 5000;
                    webBuilder.UseUrls($"http://*:{port}", $"https://*:{port + 1}");
                });
    }
}
