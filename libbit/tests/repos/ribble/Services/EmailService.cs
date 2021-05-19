using System.Threading.Tasks;
using Microsoft.AspNetCore.Identity.UI.Services;
public class EmailService : IEmailSender
{
    public Task SendEmailAsync(string email, string subject, string htmlMessage)
    {
        throw new System.NotImplementedException();
    }
}